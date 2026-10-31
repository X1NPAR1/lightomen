// tray.rs: System Tray companion for LightOmen Linux
// Uses ksni (StatusNotifierItem) for native Wayland / X11 tray support.

mod icon_data;
mod i18n;

use i18n::Language;
use ksni::{menu::*, MenuItem, Status, ToolTip, Tray, TrayService};
use lightomen_common::{
    DaemonStatus, IpcMessage, ALT_SOCKET_PATH, CMD_GET_STATUS, CMD_SET_FAN_MODE,
    CMD_SET_POWER_PROFILE, CMD_SPAWN_GUI, FAN_MODE_AUTO, FAN_MODE_CURVE, FAN_MODE_MAX,
    POWER_PROFILE_BALANCED, POWER_PROFILE_ECO, POWER_PROFILE_PERFORMANCE, SOCKET_PATH,
};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

struct LightOmenTray {
    cpu_temp: u16,
    gpu_temp: u16,
    power_profile: u8,
    fan_mode: u8,
    on_battery: u8,
    battery_pct: u8,
}

impl LightOmenTray {
    fn new() -> Self {
        Self {
            cpu_temp: 45,
            gpu_temp: 40,
            power_profile: POWER_PROFILE_BALANCED,
            fan_mode: FAN_MODE_AUTO,
            on_battery: 0,
            battery_pct: 100,
        }
    }

    fn send_ipc_cmd(cmd: u8, data: &[u8]) {
        let stream_opt = UnixStream::connect(SOCKET_PATH)
            .or_else(|_| UnixStream::connect(ALT_SOCKET_PATH));
        if let Ok(mut stream) = stream_opt {
            let mut msg = IpcMessage::new(cmd);
            let len = data.len().min(64);
            msg.data[..len].copy_from_slice(&data[..len]);
            msg.payload_len = len as u16;
            let _ = stream.write_all(msg.as_bytes());
        }
    }

    fn show_or_focus_gui() {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let gui_sock = format!("{}/lightomen-gui.sock", runtime_dir);
        if let Ok(mut stream) = UnixStream::connect(&gui_sock) {
            let _ = stream.write_all(b"FOCUS\n");
            let _ = stream.flush();
            Self::activate_window_native();
            return;
        }
        Self::send_ipc_cmd(CMD_SPAWN_GUI, &[]);
    }

    fn activate_window_native() {
        let script = r#"
            let clients = workspace.windowList();
            for (let i = 0; i < clients.length; i++) {
                let c = clients[i];
                if (c.resourceClass === 'lightomen' || c.caption.indexOf('LightOmen') !== -1) {
                    c.minimized = false;
                    workspace.activeWindow = c;
                    break;
                }
            }
        "#;
        let mut temp = std::env::temp_dir();
        temp.push("lightomen_focus_tray.js");
        if let Ok(mut f) = std::fs::File::create(&temp) {
            let _ = f.write_all(script.as_bytes());
            let _ = f.flush();
            if let Ok(output) = std::process::Command::new("qdbus6")
                .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", temp.to_str().unwrap_or(""), "lightomen_focus_tray"])
                .output()
            {
                let sid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !sid.is_empty() {
                    let _ = std::process::Command::new("qdbus6")
                        .args(["org.kde.KWin", &format!("/Scripting/Script{}", sid), "org.kde.kwin.Script.run"])
                        .output();
                    let _ = std::process::Command::new("qdbus6")
                        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", "lightomen_focus_tray"])
                        .output();
                }
            }
            let _ = std::fs::remove_file(&temp);
        }

        let _ = std::process::Command::new("wmctrl")
            .args(["-x", "-a", "lightomen"])
            .spawn();
    }

    fn fetch_status() -> Option<DaemonStatus> {
        let stream_opt = UnixStream::connect(SOCKET_PATH)
            .or_else(|_| UnixStream::connect(ALT_SOCKET_PATH));
        if let Ok(mut stream) = stream_opt {
            let msg = IpcMessage::new(CMD_GET_STATUS);
            if stream.write_all(msg.as_bytes()).is_ok() {
                let mut buf = [0u8; std::mem::size_of::<DaemonStatus>()];
                if stream.read_exact(&mut buf).is_ok() {
                    return DaemonStatus::from_bytes(&buf);
                }
            }
        }
        None
    }
}

impl Tray for LightOmenTray {
    fn id(&self) -> String {
        "lightomen-tray".into()
    }

    fn icon_name(&self) -> String {
        "lightomen".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![ksni::Icon {
            width: 32,
            height: 32,
            data: icon_data::ICON_32_ARGB.to_vec(),
        }]
    }

    fn title(&self) -> String {
        let lang = Language::load();
        let pwr = if self.on_battery == 1 {
            if lang.is_tr() { format!(" (Batarya %{})", self.battery_pct) } else { format!(" (Bat {}%)", self.battery_pct) }
        } else {
            "".to_string()
        };
        let sleep_txt = if lang.is_tr() { "Uyku" } else { "Sleep" };
        if self.gpu_temp == 0 {
            format!("LightOmen: CPU {}°C | GPU {}{}", self.cpu_temp, sleep_txt, pwr)
        } else {
            format!("LightOmen: CPU {}°C | GPU {}°C{}", self.cpu_temp, self.gpu_temp, pwr)
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let lang = Language::load();
        let gpu_str = if self.gpu_temp == 0 {
            if lang.is_tr() { "Uyku (D3cold · 0 W)".to_string() } else { "Sleep (D3cold · 0 W)".to_string() }
        } else {
            format!("{}°C", self.gpu_temp)
        };
        let pwr_str = if self.on_battery == 1 {
            if lang.is_tr() { format!("Batarya (%{})", self.battery_pct) } else { format!("Battery ({}%)", self.battery_pct) }
        } else {
            if lang.is_tr() { "AC Bağlı".to_string() } else { "AC Online".to_string() }
        };

        let desc = if lang.is_tr() {
            format!(
                "CPU: {}°C | GPU: {}\nGüç: {}\nProfil: {}\nFan Modu: {}",
                self.cpu_temp,
                gpu_str,
                pwr_str,
                match self.power_profile {
                    0 => "Eco",
                    1 => "Balanced",
                    _ => "Performance",
                },
                match self.fan_mode {
                    0 => "Auto",
                    1 => "Max",
                    _ => "Grafik",
                }
            )
        } else {
            format!(
                "CPU: {}°C | GPU: {}\nPower: {}\nProfile: {}\nFan Mode: {}",
                self.cpu_temp,
                gpu_str,
                pwr_str,
                match self.power_profile {
                    0 => "Eco",
                    1 => "Balanced",
                    _ => "Performance",
                },
                match self.fan_mode {
                    0 => "Auto",
                    1 => "Max",
                    _ => "Curve",
                }
            )
        };

        ToolTip {
            title: "LightOmen Linux".into(),
            description: desc,
            icon_name: "lightomen".into(),
            icon_pixmap: vec![ksni::Icon {
                width: 32,
                height: 32,
                data: icon_data::ICON_32_ARGB.to_vec(),
            }],
        }
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        Self::show_or_focus_gui();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let lang = Language::load();
        let cur_power = self.power_profile;
        let cur_fan = self.fan_mode;

        let open_label = if lang.is_tr() { "LightOmen'i Aç" } else { "Open LightOmen" };
        let power_label = if lang.is_tr() { "Güç Profili" } else { "Power Profile" };
        let fan_label = if lang.is_tr() { "Fan Modu" } else { "Fan Mode" };
        let curve_label = if lang.is_tr() { "Grafik" } else { "Curve" };
        let quit_label = if lang.is_tr() { "Tepsiden Çıkış" } else { "Quit Tray" };

        vec![
            StandardItem {
                label: open_label.into(),
                activate: Box::new(|_| {
                    LightOmenTray::show_or_focus_gui();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: power_label.into(),
                submenu: vec![
                    CheckmarkItem {
                        label: "Eco".into(),
                        checked: cur_power == POWER_PROFILE_ECO,
                        activate: Box::new(|tray: &mut Self| {
                            tray.power_profile = POWER_PROFILE_ECO;
                            LightOmenTray::send_ipc_cmd(CMD_SET_POWER_PROFILE, &[POWER_PROFILE_ECO]);
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: "Balanced".into(),
                        checked: cur_power == POWER_PROFILE_BALANCED,
                        activate: Box::new(|tray: &mut Self| {
                            tray.power_profile = POWER_PROFILE_BALANCED;
                            LightOmenTray::send_ipc_cmd(CMD_SET_POWER_PROFILE, &[POWER_PROFILE_BALANCED]);
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: "Performance".into(),
                        checked: cur_power == POWER_PROFILE_PERFORMANCE,
                        activate: Box::new(|tray: &mut Self| {
                            tray.power_profile = POWER_PROFILE_PERFORMANCE;
                            LightOmenTray::send_ipc_cmd(CMD_SET_POWER_PROFILE, &[POWER_PROFILE_PERFORMANCE]);
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: fan_label.into(),
                submenu: vec![
                    CheckmarkItem {
                        label: "Auto".into(),
                        checked: cur_fan == FAN_MODE_AUTO,
                        activate: Box::new(|tray: &mut Self| {
                            tray.fan_mode = FAN_MODE_AUTO;
                            LightOmenTray::send_ipc_cmd(CMD_SET_FAN_MODE, &[FAN_MODE_AUTO]);
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: "Max".into(),
                        checked: cur_fan == FAN_MODE_MAX,
                        activate: Box::new(|tray: &mut Self| {
                            tray.fan_mode = FAN_MODE_MAX;
                            LightOmenTray::send_ipc_cmd(CMD_SET_FAN_MODE, &[FAN_MODE_MAX]);
                        }),
                        ..Default::default()
                    }
                    .into(),
                    CheckmarkItem {
                        label: curve_label.into(),
                        checked: cur_fan == FAN_MODE_CURVE,
                        activate: Box::new(|tray: &mut Self| {
                            tray.fan_mode = FAN_MODE_CURVE;
                            LightOmenTray::send_ipc_cmd(CMD_SET_FAN_MODE, &[FAN_MODE_CURVE]);
                        }),
                        ..Default::default()
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: quit_label.into(),
                activate: Box::new(|_| {
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn apply_refresh_rate(hz: u32) {
    let mode_arg = format!("1920x1080@{}", hz);
    let _ = std::process::Command::new("kscreen-doctor")
        .arg(format!("output.eDP-1.mode.{}", mode_arg))
        .arg(format!("output.1.mode.{}", mode_arg))
        .spawn();
}

fn main() {
    // Force Mesa/integrated AMD graphics: never probe or wake NVIDIA dGPU
    std::env::remove_var("DRI_PRIME");
    std::env::set_var("__NV_PRIME_RENDER_OFFLOAD", "0");
    std::env::set_var("__GLX_VENDOR_LIBRARY_NAME", "mesa");

    let tray = LightOmenTray::new();
    let service = TrayService::new(tray);
    let handle = service.handle();

    thread::Builder::new()
        .name("lightomen-tray-service".into())
        .spawn(move || {
            let _ = service.run();
        })
        .expect("failed to spawn tray service");

    let mut prev_on_battery: Option<bool> = None;

    loop {
        if let Some(status) = LightOmenTray::fetch_status() {
            let on_bat = status.on_battery == 1;
            if prev_on_battery != Some(on_bat) {
                if status.config.low_hz_on_battery() == 1 {
                    apply_refresh_rate(if on_bat { 60 } else { 144 });
                }
                prev_on_battery = Some(on_bat);
            }

            handle.update(|tray| {
                tray.cpu_temp = status.telemetry.cpu_temp;
                tray.gpu_temp = status.telemetry.gpu_temp;
                tray.power_profile = status.config.power_profile;
                tray.fan_mode = status.config.fan_mode;
                tray.on_battery = status.on_battery;
                tray.battery_pct = status.battery_pct;
            });
        }
        thread::sleep(Duration::from_secs(2));
    }
}
