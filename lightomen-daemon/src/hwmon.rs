// hwmon.rs: Persistent-FD Hardware Backend
// Pure std, zero tokio/serde, zero glob overhead, seek(0) stack buffer reads.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Instant;

pub struct HwmonBackend {
    cpu_temp_file: Option<File>,
    fan1_input_file: Option<File>,
    fan2_input_file: Option<File>,
    fan1_target_path: Option<PathBuf>,
    fan2_target_path: Option<PathBuf>,
    fan_pwm_enable_path: Option<PathBuf>,
    platform_profile_path: Option<PathBuf>,
    gpu_tgp_path: Option<PathBuf>,
    gpu_ppab_path: Option<PathBuf>,
    gpu_mux_path: Option<PathBuf>,
    graphics_mode_path: Option<PathBuf>,
    rapl_energy_file: Option<File>,
    last_energy_uj: u64,
    last_energy_time: Instant,
    gpu_pci_status_file: Option<File>,
    pub max_rpm: u16,
    pub board_id: [u8; 8],
    last_applied_mode: Option<u8>,
    last_target1: u16,
    last_target2: u16,
}

impl HwmonBackend {
    pub fn new() -> Self {
        let board_id = Self::read_board_id();
        let paths = Self::discover_hwmon_paths();

        let cpu_temp_file = paths.cpu_temp.and_then(|p| File::open(p).ok());
        let fan1_input_file = paths.fan1_input.and_then(|p| File::open(p).ok());
        let fan2_input_file = paths.fan2_input.and_then(|p| File::open(p).ok());

        let fan1_target_path = paths.fan1_target;
        let fan2_target_path = paths.fan2_target;
        let fan_pwm_enable_path = paths.fan_pwm_enable;

        let platform_profile_path = {
            let p = PathBuf::from("/sys/firmware/acpi/platform_profile");
            if p.exists() { Some(p) } else { None }
        };

        let gpu_tgp_path = {
            let p = PathBuf::from("/sys/devices/platform/hp-wmi/gpu_tgp");
            if p.exists() { Some(p) } else { None }
        };

        let gpu_ppab_path = {
            let p = PathBuf::from("/sys/devices/platform/hp-wmi/gpu_ppab");
            if p.exists() { Some(p) } else { None }
        };

        let gpu_mux_path = {
            let p = PathBuf::from("/sys/devices/platform/hp-wmi/gpu_mux_mode");
            if p.exists() { Some(p) } else { None }
        };

        let graphics_mode_path = {
            let p = PathBuf::from("/sys/devices/platform/hp-wmi/graphics_mode");
            if p.exists() { Some(p) } else { None }
        };

        let max_rpm = paths.max_rpm.unwrap_or(5800).clamp(4000, 7000);

        let rapl_energy_file = {
            let p1 = PathBuf::from("/sys/class/powercap/intel-rapl:0/energy_uj");
            let p2 = PathBuf::from("/sys/devices/virtual/powercap/intel-rapl/intel-rapl:0/energy_uj");
            File::open(p1).or_else(|_| File::open(p2)).ok()
        };

        let gpu_pci_status_file = {
            let p = PathBuf::from("/sys/bus/pci/devices/0000:01:00.0/power/runtime_status");
            if p.exists() {
                File::open(p).ok()
            } else {
                let mut found = None;
                if let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") {
                    for entry in entries.flatten() {
                        let path = entry.path().join("power/runtime_status");
                        let class_path = entry.path().join("class");
                        if path.exists() && class_path.exists() {
                            if let Ok(c) = fs::read_to_string(&class_path) {
                                if c.trim().starts_with("0x03") {
                                    let vendor_path = entry.path().join("vendor");
                                    if let Ok(v) = fs::read_to_string(&vendor_path) {
                                        if v.trim().contains("0x10de") {
                                            found = File::open(path).ok();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                found
            }
        };

        Self {
            cpu_temp_file,
            fan1_input_file,
            fan2_input_file,
            fan1_target_path,
            fan2_target_path,
            fan_pwm_enable_path,
            platform_profile_path,
            gpu_tgp_path,
            gpu_ppab_path,
            gpu_mux_path,
            graphics_mode_path,
            rapl_energy_file,
            last_energy_uj: 0,
            last_energy_time: Instant::now(),
            gpu_pci_status_file,
            max_rpm,
            board_id,
            last_applied_mode: None,
            last_target1: 0,
            last_target2: 0,
        }
    }

    fn read_board_id() -> [u8; 8] {
        let mut id = [0u8; 8];
        if let Ok(mut f) = File::open("/sys/class/dmi/id/board_name") {
            let mut buf = [0u8; 16];
            if let Ok(n) = f.read(&mut buf) {
                let trimmed = buf[..n].trim_ascii();
                let len = trimmed.len().min(7);
                id[..len].copy_from_slice(&trimmed[..len]);
            }
        }
        if id[0] == 0 {
            id[..4].copy_from_slice(b"8BD4");
        }
        id
    }

    fn discover_hwmon_paths() -> DiscoveredPaths {
        let mut res = DiscoveredPaths::default();

        if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
            for entry in entries.flatten() {
                let p = entry.path();
                let name_file = p.join("name");
                if let Ok(name_str) = fs::read_to_string(&name_file) {
                    let name = name_str.trim();
                    if name == "k10temp" || name == "coretemp" || name == "zenpower" {
                        let t = p.join("temp1_input");
                        if t.exists() {
                            res.cpu_temp = Some(t);
                        }
                    } else if name == "hp" || name == "hp_wmi" || name == "omen" {
                        let f1 = p.join("fan1_input");
                        let f2 = p.join("fan2_input");
                        let t1 = p.join("fan1_target");
                        let t2 = p.join("fan2_target");
                        let m1 = p.join("fan1_max");
                        let pwm_en = p.join("pwm1_enable");
                        let pwm_duty = p.join("pwm1");

                        if f1.exists() {
                            res.fan1_input = Some(f1);
                        }
                        if f2.exists() {
                            res.fan2_input = Some(f2);
                        }
                        if t1.exists() {
                            res.fan1_target = Some(t1);
                        }
                        if t2.exists() {
                            res.fan2_target = Some(t2);
                        }
                        if m1.exists() {
                            if let Ok(m_str) = fs::read_to_string(&m1) {
                                if let Ok(val) = m_str.trim().parse::<u16>() {
                                    if val >= 4000 {
                                        res.max_rpm = Some(val);
                                    }
                                }
                            }
                        }
                        if pwm_en.exists() {
                            res.fan_pwm_enable = Some(pwm_en);
                        }
                        if pwm_duty.exists() {
                            res.fan_pwm_duty = Some(pwm_duty);
                        }
                    }
                }
            }
        }

        res
    }

    fn read_fd_number(file_opt: &mut Option<File>) -> Option<i64> {
        let file = file_opt.as_mut()?;
        if file.seek(SeekFrom::Start(0)).is_err() {
            return None;
        }
        let mut buf = [0u8; 64];
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            return None;
        }
        let s = std::str::from_utf8(&buf[..n]).ok()?;
        s.trim().parse::<i64>().ok()
    }

    fn write_sysfs_string(path_opt: &Option<PathBuf>, s: &str) -> bool {
        if let Some(path) = path_opt {
            if fs::write(path, s.as_bytes()).is_ok() {
                return true;
            }
        }
        false
    }

    pub fn read_cpu_temp(&mut self) -> Option<u16> {
        Self::read_fd_number(&mut self.cpu_temp_file).map(|milli| {
            let c = (milli / 1000) as u16;
            if c > 0 && c < 125 {
                c
            } else {
                0
            }
        })
    }

    /// Check if NVIDIA dGPU is currently in D3cold (suspended) power-saving sleep.
    /// Safely inspects Linux kernel runtime PM in memory without waking the PCIe bus.
    pub fn is_gpu_suspended(&mut self) -> bool {
        if let Some(ref mut f) = self.gpu_pci_status_file {
            if f.seek(SeekFrom::Start(0)).is_ok() {
                let mut buf = [0u8; 32];
                if let Ok(n) = f.read(&mut buf) {
                    let s = std::str::from_utf8(&buf[..n]).unwrap_or("");
                    return s.trim() == "suspended";
                }
            }
        } else {
            // Lazy fallback open if not open at initialization
            let p = PathBuf::from("/sys/bus/pci/devices/0000:01:00.0/power/runtime_status");
            if let Ok(mut f) = File::open(&p) {
                let mut buf = [0u8; 32];
                if let Ok(n) = f.read(&mut buf) {
                    let s = std::str::from_utf8(&buf[..n]).unwrap_or("");
                    let is_susp = s.trim() == "suspended";
                    self.gpu_pci_status_file = Some(f);
                    return is_susp;
                }
            }
        }
        false
    }

    /// Read dynamic fan RPM directly from hp-wmi hwmon (fan1_input, fan2_input)
    pub fn read_fans(&mut self) -> (Option<u16>, Option<u16>) {
        let f1 = Self::read_fd_number(&mut self.fan1_input_file).and_then(|r| {
            if (0..10000).contains(&r) {
                Some(r as u16)
            } else {
                None
            }
        });
        let f2 = Self::read_fd_number(&mut self.fan2_input_file).and_then(|r| {
            if (0..10000).contains(&r) {
                Some(r as u16)
            } else {
                None
            }
        });
        (f1, f2)
    }

    pub fn set_platform_profile(&mut self, profile_str: &str) -> bool {
        Self::write_sysfs_string(&self.platform_profile_path, profile_str)
    }

    pub fn set_gpu_tgp(&mut self, enable: bool) -> bool {
        Self::write_sysfs_string(&self.gpu_tgp_path, if enable { "1\n" } else { "0\n" })
    }

    pub fn set_gpu_ppab(&mut self, enable: bool) -> bool {
        Self::write_sysfs_string(&self.gpu_ppab_path, if enable { "1\n" } else { "0\n" })
    }

    #[allow(dead_code)]
    pub fn get_mux_mode(&mut self) -> Option<u8> {
        self.gpu_mux_path.as_ref().and_then(|p| {
            fs::read_to_string(p).ok().and_then(|s| s.trim().parse::<u8>().ok())
        })
    }

    pub fn set_mux_mode(&mut self, mode: u8) -> bool {
        let s = format!("{}\n", mode);
        let ok1 = Self::write_sysfs_string(&self.gpu_mux_path, &s);
        let ok2 = Self::write_sysfs_string(&self.graphics_mode_path, &s);
        ok1 || ok2
    }

    /// Apply fan speeds and modes via hp-wmi hwmon targets, pwm1_enable
    pub fn apply_fan_mode(&mut self, mode: u8, target1_rpm: u16, target2_rpm: u16) {
        match mode {
            1 => {
                // FAN_MODE_MAX (1):
                // Directly write 0 to pwm1_enable. In hp-wmi, 0 triggers PWM_MODE_MAX
                if self.last_applied_mode != Some(1) {
                    let _ = Self::write_sysfs_string(&self.fan_pwm_enable_path, "0\n");
                    self.last_applied_mode = Some(1);
                }
            }
            2 => {
                // FAN_MODE_CURVE (2):
                // 0 RPM strictly stops the fans (Zero-RPM mode / Silent idle).
                // Above 0, clamp to operational floor (1800 RPM..max_rpm).
                let t1 = if target1_rpm == 0 { 0 } else { target1_rpm.clamp(1800, self.max_rpm) };
                let t2 = if target2_rpm == 0 { 0 } else { target2_rpm.clamp(1800, self.max_rpm) };
                let mode_changed = self.last_applied_mode != Some(2);
                let t1_diff = (t1 as i32 - self.last_target1 as i32).abs();
                let t2_diff = (t2 as i32 - self.last_target2 as i32).abs();

                // Only write when mode changed or targets shifted by >= 40 RPM (or transition to/from 0)
                if mode_changed || t1_diff >= 40 || t2_diff >= 40 || (t1 == 0 && self.last_target1 != 0) || (t1 != 0 && self.last_target1 == 0) {
                    let _ = Self::write_sysfs_string(&self.fan1_target_path, &format!("{}\n", t1));
                    let _ = Self::write_sysfs_string(&self.fan2_target_path, &format!("{}\n", t2));
                    self.last_applied_mode = Some(2);
                    self.last_target1 = t1;
                    self.last_target2 = t2;
                }
            }
            _ => {
                // FAN_MODE_AUTO (0):
                if self.last_applied_mode != Some(0) {
                    if self.last_applied_mode == Some(1) {
                        // Unlatch MAX mode before giving control back to BIOS
                        let _ = Self::write_sysfs_string(&self.fan1_target_path, "2000\n");
                        let _ = Self::write_sysfs_string(&self.fan2_target_path, "2000\n");
                    }
                    let _ = Self::write_sysfs_string(&self.fan_pwm_enable_path, "2\n");
                    self.last_applied_mode = Some(0);
                }
            }
        }
    }

    /// Reads live CPU wattage from RAPL energy_uj (AMD/Intel energy driver)
    pub fn read_cpu_power_mw(&mut self, power_profile: u8) -> u32 {
        if let Some(ref mut f) = self.rapl_energy_file {
            if f.seek(SeekFrom::Start(0)).is_ok() {
                let mut buf = [0u8; 32];
                if let Ok(n) = f.read(&mut buf) {
                    if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                        if let Ok(cur_uj) = s.trim().parse::<u64>() {
                            let elapsed = self.last_energy_time.elapsed().as_millis() as u64;
                            if self.last_energy_uj > 0 && cur_uj > self.last_energy_uj && elapsed > 200 {
                                let delta_uj = cur_uj - self.last_energy_uj;
                                let watts_f = (delta_uj as f64) / (elapsed as f64 * 1000.0);
                                self.last_energy_uj = cur_uj;
                                self.last_energy_time = Instant::now();
                                return (watts_f * 1000.0).clamp(1000.0, 150000.0) as u32;
                            } else if self.last_energy_uj == 0 || cur_uj < self.last_energy_uj {
                                self.last_energy_uj = cur_uj;
                                self.last_energy_time = Instant::now();
                            }
                        }
                    }
                }
            }
        }
        match power_profile {
            0 => 12000,
            1 => 25000,
            _ => 45000,
        }
    }
}

#[derive(Default)]
struct DiscoveredPaths {
    cpu_temp: Option<PathBuf>,
    fan1_input: Option<PathBuf>,
    fan2_input: Option<PathBuf>,
    fan1_target: Option<PathBuf>,
    fan2_target: Option<PathBuf>,
    fan_pwm_enable: Option<PathBuf>,
    fan_pwm_duty: Option<PathBuf>,
    max_rpm: Option<u16>,
}
