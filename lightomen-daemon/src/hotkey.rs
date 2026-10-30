// hotkey.rs: Blocking evdev listener for physical OMEN hotkey & Hotkey Learning
// Listens to /dev/input/event* and triggers Spawn-on-Demand GUI.

use evdev::{Device, EventType};
use lightomen_common::{DaemonStatus, CONFIG_FILE};
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

static LEARNING_ACTIVE: AtomicBool = AtomicBool::new(false);
static LEARNING_START_SECS: AtomicU64 = AtomicU64::new(0);

pub struct HotkeyMonitor;

impl HotkeyMonitor {
    pub fn start(status: Arc<Mutex<DaemonStatus>>) {
        thread::Builder::new()
            .name("lightomen-hotkey".into())
            .spawn(move || {
                Self::run_loop(status);
            })
            .expect("failed to spawn hotkey thread");
    }

    pub fn start_learning() {
        LEARNING_START_SECS.store(now_secs(), Ordering::SeqCst);
        LEARNING_ACTIVE.store(true, Ordering::SeqCst);
        eprintln!("lightomen-daemon: Entered OMEN key learning mode (timeout 10s)");
    }

    pub fn cancel_learning() {
        LEARNING_ACTIVE.store(false, Ordering::SeqCst);
        eprintln!("lightomen-daemon: Canceled OMEN key learning mode");
    }

    pub fn check_timeout() -> bool {
        if LEARNING_ACTIVE.load(Ordering::SeqCst) {
            let start = LEARNING_START_SECS.load(Ordering::SeqCst);
            if now_secs().saturating_sub(start) >= 10 {
                LEARNING_ACTIVE.store(false, Ordering::SeqCst);
                eprintln!("lightomen-daemon: OMEN key learning timed out (10s)");
                return true;
            }
        }
        false
    }

    fn run_loop(status: Arc<Mutex<DaemonStatus>>) {
        loop {
            let devices = Self::find_hotkey_devices();
            if devices.is_empty() {
                thread::sleep(Duration::from_secs(2));
                continue;
            }

            let mut threads = Vec::new();
            let running = Arc::new(AtomicBool::new(true));

            for dev in devices {
                let r = running.clone();
                let st = status.clone();

                let handle = thread::spawn(move || {
                    let mut d = dev;
                    while r.load(Ordering::Relaxed) {
                        match d.fetch_events() {
                            Ok(events) => {
                                for ev in events {
                                    if ev.event_type() == EventType::KEY && ev.value() == 1 {
                                        let code = ev.code();

                                        if LEARNING_ACTIVE.load(Ordering::SeqCst) {
                                            // Check timeout
                                            let start = LEARNING_START_SECS.load(Ordering::SeqCst);
                                            if now_secs().saturating_sub(start) >= 10 {
                                                LEARNING_ACTIVE.store(false, Ordering::SeqCst);
                                                let mut s = st.lock().unwrap();
                                                s.is_learning_key = 0;
                                                continue;
                                            }

                                            // Ignore modifiers alone (Shift, Ctrl, Alt, Meta/Super)
                                            if code == 42 || code == 54 || code == 29 || code == 97 || code == 56 || code == 100 || code == 125 || code == 126 {
                                                continue;
                                            }

                                            // Key successfully learned!
                                            eprintln!("lightomen-daemon: Successfully captured OMEN key code: {}", code);
                                            {
                                                let mut s = st.lock().unwrap();
                                                s.config.hotkey_code = code;
                                                s.last_learned_key = code;
                                                s.is_learning_key = 0;
                                                let _ = s.config.save_to_file(CONFIG_FILE);
                                            }
                                            LEARNING_ACTIVE.store(false, Ordering::SeqCst);
                                        } else {
                                            let target_code = {
                                                let s = st.lock().unwrap();
                                                s.config.hotkey_code
                                            };
                                            let is_target = target_code != 0 && code == target_code;
                                            // 149: Configured default OMEN keycode
                                            // 148: KEY_PROG1 (OMEN key standard)
                                            // 200: XF86Launch2
                                            // 194: KEY_F24
                                            if is_target || code == 149 || code == 148 || code == 200 || code == 194 {
                                                eprintln!("lightomen-daemon: OMEN hotkey pressed ({}), toggling GUI", code);
                                                Self::toggle_gui();
                                            }
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                });
                threads.push(handle);
            }

            for t in threads {
                let _ = t.join();
            }

            thread::sleep(Duration::from_secs(1));
        }
    }

    fn find_hotkey_devices() -> Vec<Device> {
        let mut devices = Vec::new();
        if let Ok(entries) = fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("event") {
                        if let Ok(dev) = Device::open(&path) {
                            let dev_name = dev.name().unwrap_or("").to_lowercase();
                            if dev_name.contains("mouse")
                                || dev_name.contains("touchpad")
                                || dev_name.contains("speaker")
                                || dev_name.contains("lid")
                            {
                                continue;
                            }
                            if dev_name.contains("hp wmi")
                                || dev_name.contains("hotkey")
                                || dev_name.contains("keyboard")
                                || dev_name.contains("control")
                            {
                                devices.push(dev);
                            }
                        }
                    }
                }
            }
        }
        devices
    }

    /// Spawn or toggle the GUI: if already open, terminate it; if closed, spawn fresh.
    pub fn toggle_gui() {
        // Check if lightomen-gui or ohman-gui is already running
        let is_running = Command::new("sh")
            .arg("-c")
            .arg("pgrep -x lightomen-gui || pgrep -x ohman-gui")
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);

        if is_running {
            let _ = Command::new("pkill").arg("-x").arg("lightomen-gui").output();
            let _ = Command::new("pkill").arg("-x").arg("ohman-gui").output();
        } else {
            Self::spawn_gui();
        }
    }

    pub fn detect_user_session() -> (String, u32, String, String) {
        let mut target_uid = 1000u32;
        let mut target_user = "zyron".to_string();
        let mut runtime_dir = format!("/run/user/{}", target_uid);

        if let Ok(entries) = fs::read_dir("/run/user") {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if let Ok(uid) = name.parse::<u32>() {
                        if uid >= 1000 {
                            target_uid = uid;
                            runtime_dir = format!("/run/user/{}", uid);
                            if let Ok(content) = fs::read_to_string("/etc/passwd") {
                                for line in content.lines() {
                                    let fields: Vec<&str> = line.split(':').collect();
                                    if fields.len() >= 3 && fields[2] == name {
                                        target_user = fields[0].to_string();
                                        break;
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }

        let mut wayland_disp = "wayland-0".to_string();
        if let Ok(entries) = fs::read_dir(&runtime_dir) {
            for entry in entries.flatten() {
                if let Some(fname) = entry.file_name().to_str() {
                    if fname.starts_with("wayland-") && !fname.ends_with(".lock") {
                        wayland_disp = fname.to_string();
                        break;
                    }
                }
            }
        }

        (target_user, target_uid, runtime_dir, wayland_disp)
    }

    /// Set display refresh rate in user session via kscreen-doctor
    pub fn set_display_refresh_rate(hz: u32) {
        let (target_user, _, _, _) = Self::detect_user_session();
        let machine_arg = format!("--machine={}@.host", target_user);
        let mode_arg = format!("1920x1080@{}", hz);

        let _ = Command::new("systemd-run")
            .args([
                "--user",
                &machine_arg,
                "--quiet",
                "kscreen-doctor",
                &format!("output.1.mode.{}", mode_arg),
            ])
            .spawn();
    }

    /// Spawns lightomen-gui in the active user session on Wayland
    pub fn spawn_gui() {
        let (target_user, target_uid, runtime_dir, wayland_disp) = Self::detect_user_session();

        let binary_path = if Path::new("/usr/local/bin/lightomen-gui").exists() {
            "/usr/local/bin/lightomen-gui"
        } else if Path::new("/usr/local/bin/ohman-gui").exists() {
            "/usr/local/bin/ohman-gui"
        } else if Path::new("/usr/bin/lightomen-gui").exists() {
            "/usr/bin/lightomen-gui"
        } else {
            "lightomen-gui"
        };

        let tray_path = if Path::new("/usr/local/bin/lightomen-tray").exists() {
            "/usr/local/bin/lightomen-tray"
        } else if Path::new("/usr/local/bin/ohman-tray").exists() {
            "/usr/local/bin/ohman-tray"
        } else if Path::new("/usr/bin/lightomen-tray").exists() {
            "/usr/bin/lightomen-tray"
        } else {
            "lightomen-tray"
        };

        let dbus_addr = format!("unix:path={}/bus", runtime_dir);
        let machine_arg = format!("--machine={}@.host", target_user);

        // Ensure tray is also running in its own independent user unit
        let tray_running = Command::new("sh")
            .arg("-c")
            .arg(format!("pgrep -u {} -x lightomen-tray || pgrep -u {} -x ohman-tray", target_uid, target_uid))
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);

        if !tray_running {
            let mut tray_cmd = Command::new("systemd-run");
            tray_cmd.args([
                &machine_arg,
                "--user",
                "--unit=lightomen-tray",
                "--collect",
                tray_path,
            ]);
            tray_cmd.env("XDG_RUNTIME_DIR", &runtime_dir);
            tray_cmd.env("DBUS_SESSION_BUS_ADDRESS", &dbus_addr);
            tray_cmd.env("WAYLAND_DISPLAY", &wayland_disp);
            tray_cmd.env("DISPLAY", ":0");
            let _ = tray_cmd.spawn();
        }

        let mut cmd = Command::new("systemd-run");
        cmd.args([
            &machine_arg,
            "--user",
            "--unit=lightomen-gui",
            "--collect",
            binary_path,
        ]);
        cmd.env("XDG_RUNTIME_DIR", &runtime_dir);
        cmd.env("DBUS_SESSION_BUS_ADDRESS", &dbus_addr);
        cmd.env("WAYLAND_DISPLAY", &wayland_disp);
        cmd.env("DISPLAY", ":0");
        cmd.env("DRI_PRIME", "0");
        cmd.env("__NV_PRIME_RENDER_OFFLOAD", "0");
        cmd.env("__GLX_VENDOR_LIBRARY_NAME", "mesa");

        if cmd.spawn().is_err() {
            // Fallback: spawn binary directly under target user credentials
            let mut direct = Command::new(binary_path);
            direct.uid(target_uid);
            direct.gid(target_uid);
            direct.env("XDG_RUNTIME_DIR", &runtime_dir);
            direct.env("DBUS_SESSION_BUS_ADDRESS", &dbus_addr);
            direct.env("WAYLAND_DISPLAY", &wayland_disp);
            direct.env("DISPLAY", ":0");
            direct.env("DRI_PRIME", "0");
            direct.env("__NV_PRIME_RENDER_OFFLOAD", "0");
            direct.env("__GLX_VENDOR_LIBRARY_NAME", "mesa");
            let _ = direct.spawn();
        }
    }
}
