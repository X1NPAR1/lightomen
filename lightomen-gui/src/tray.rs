// tray.rs: System Tray companion for LightOmen Linux
// Uses ksni (StatusNotifierItem) for native Wayland / X11 tray support.

mod icon_data;

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
}

impl LightOmenTray {
    fn new() -> Self {
        Self {
            cpu_temp: 45,
            gpu_temp: 40,
            power_profile: POWER_PROFILE_BALANCED,
            fan_mode: FAN_MODE_AUTO,
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
        if self.gpu_temp == 0 {
            format!("LightOmen: CPU {}°C | GPU Sleep (0 W)", self.cpu_temp)
        } else {
            format!("LightOmen: CPU {}°C | GPU {}°C", self.cpu_temp, self.gpu_temp)
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let gpu_str = if self.gpu_temp == 0 {
            "Sleep (D3cold · 0 W)".to_string()
        } else {
            format!("{}°C", self.gpu_temp)
        };

        ToolTip {
            title: "LightOmen Linux".into(),
            description: format!(
                "CPU: {}°C | GPU: {}\nProfile: {}\nFan Mode: {}",
                self.cpu_temp,
                gpu_str,
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
            ),
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
        Self::send_ipc_cmd(CMD_SPAWN_GUI, &[]);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let cur_power = self.power_profile;
        let cur_fan = self.fan_mode;

        vec![
            StandardItem {
                label: "Open LightOmen".into(),
                activate: Box::new(|_| {
                    LightOmenTray::send_ipc_cmd(CMD_SPAWN_GUI, &[]);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Power Profile".into(),
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
                label: "Fan Mode".into(),
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
                        label: "Curve".into(),
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
                label: "Quit Tray".into(),
                activate: Box::new(|_| {
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn main() {
    // Force Mesa/integrated AMD graphics: never probe or wake NVIDIA dGPU
    std::env::set_var("DRI_PRIME", "0");
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

    loop {
        if let Some(status) = LightOmenTray::fetch_status() {
            handle.update(|tray| {
                tray.cpu_temp = status.telemetry.cpu_temp;
                tray.gpu_temp = status.telemetry.gpu_temp;
                tray.power_profile = status.config.power_profile;
                tray.fan_mode = status.config.fan_mode;
            });
        }
        thread::sleep(Duration::from_secs(2));
    }
}
