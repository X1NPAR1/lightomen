// ec.rs: Direct Embedded Controller I/O via ports 0x62 and 0x66
// Zero-bloat, direct ACPI handshake for HP OMEN / Victus EC registers

use std::sync::Mutex;
use std::thread;
use std::time::Duration;

const DATA_PORT: u16 = 0x62;
const CMD_PORT: u16 = 0x66;

const CMD_READ: u8 = 0x80;
const CMD_WRITE: u8 = 0x81;

const OBF: u8 = 0x01; // Output buffer full (ready to read from data port)
const IBF: u8 = 0x02; // Input buffer full (controller is processing)

// Register map for HP OMEN / Victus EC
pub const REG_CPU_TEMP: u8 = 0x57; // CPUT: degrees C
pub const REG_GPU_TEMP: u8 = 0xB7; // GPTM: degrees C
pub const REG_FAN1_RPM: u8 = 0xB0; // Tachometer 1 (low byte)
pub const REG_FAN2_RPM: u8 = 0xB2; // Tachometer 2 (low byte)

#[allow(dead_code)]
pub const REG_MANUAL: u8 = 0x62; // OMCC
#[allow(dead_code)]
pub const MANUAL_ON: u8 = 0x06;
#[allow(dead_code)]
pub const MANUAL_OFF: u8 = 0x00;

#[allow(dead_code)]
pub const REG_COUNTDOWN: u8 = 0x63;
#[allow(dead_code)]
pub const COUNTDOWN_HOLD: u8 = 0xFF;

pub const REG_FAN_SET1_RPM100: u8 = 0x34; // SRP1: rpm/100
pub const REG_FAN_SET2_RPM100: u8 = 0x35; // SRP2: rpm/100
pub const REG_FAN_SET1_PCT: u8 = 0x2C;    // XSS1: percentage
pub const REG_FAN_SET2_PCT: u8 = 0x2D;    // XSS2: percentage
#[allow(dead_code)]
pub const REG_FAN_BOOST: u8 = 0xEC;       // Fan boost hardware companion

static EC_LOCK: Mutex<()> = Mutex::new(());

pub struct EmbeddedController {
    initialized: bool,
}

impl EmbeddedController {
    pub fn new() -> Self {
        let ok = unsafe {
            libc::ioperm(DATA_PORT as libc::c_ulong, 1, 1) == 0
                && libc::ioperm(CMD_PORT as libc::c_ulong, 1, 1) == 0
        };
        Self { initialized: ok }
    }

    #[inline]
    unsafe fn inb(port: u16) -> u8 {
        let val: u8;
        std::arch::asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
        val
    }

    #[inline]
    unsafe fn outb(port: u16, val: u8) {
        std::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
    }

    fn wait_for(&self, bit: u8, set: bool) -> bool {
        for i in 0..400 {
            let s = unsafe { Self::inb(CMD_PORT) };
            if ((s & bit) != 0) == set {
                return true;
            }
            if i >= 20 {
                thread::sleep(Duration::from_micros(200));
            }
        }
        false
    }

    fn drain(&self) {
        let s = unsafe { Self::inb(CMD_PORT) };
        if (s & OBF) != 0 {
            unsafe { Self::inb(DATA_PORT) };
        }
    }

    pub fn read_reg(&self, reg: u8) -> Result<u8, ()> {
        if !self.initialized {
            return Err(());
        }
        let _guard = EC_LOCK.lock().unwrap();

        self.drain();
        if !self.wait_for(IBF, false) {
            return Err(());
        }
        unsafe { Self::outb(CMD_PORT, CMD_READ) };
        if !self.wait_for(IBF, false) {
            return Err(());
        }
        unsafe { Self::outb(DATA_PORT, reg) };
        if !self.wait_for(OBF, true) {
            return Err(());
        }
        Ok(unsafe { Self::inb(DATA_PORT) })
    }

    pub fn write_reg(&self, reg: u8, val: u8) -> Result<(), ()> {
        if !self.initialized {
            return Err(());
        }
        let _guard = EC_LOCK.lock().unwrap();

        if !self.wait_for(IBF, false) {
            return Err(());
        }
        unsafe { Self::outb(CMD_PORT, CMD_WRITE) };
        if !self.wait_for(IBF, false) {
            return Err(());
        }
        unsafe { Self::outb(DATA_PORT, reg) };
        if !self.wait_for(IBF, false) {
            return Err(());
        }
        unsafe { Self::outb(DATA_PORT, val) };
        if !self.wait_for(IBF, false) {
            return Err(());
        }
        Ok(())
    }

    /// Read GPU temperature directly from EC thermistor register 0xB7
    pub fn read_gpu_temp(&self) -> Option<u16> {
        if let Ok(temp) = self.read_reg(REG_GPU_TEMP) {
            if temp > 0 && temp < 120 {
                return Some(temp as u16);
            }
        }
        None
    }

    /// Read CPU temperature directly from EC register 0x57
    pub fn read_cpu_temp(&self) -> Option<u16> {
        if let Ok(temp) = self.read_reg(REG_CPU_TEMP) {
            if temp > 0 && temp < 120 {
                return Some(temp as u16);
            }
        }
        None
    }

    /// Read Fan 1 & 2 RPM from tachometer registers 0xB0 and 0xB2
    /// Strictly validates that speed is in plausible real RPM range (>= 300 RPM)
    pub fn read_fan_rpms(&self) -> (Option<u16>, Option<u16>) {
        let f1 = match (self.read_reg(REG_FAN1_RPM), self.read_reg(REG_FAN1_RPM + 1)) {
            (Ok(lo), Ok(hi)) => {
                let raw = (lo as u16) | ((hi as u16) << 8);
                if (300..9000).contains(&raw) {
                    Some(raw)
                } else {
                    None
                }
            }
            _ => None,
        };

        let f2 = match (self.read_reg(REG_FAN2_RPM), self.read_reg(REG_FAN2_RPM + 1)) {
            (Ok(lo), Ok(hi)) => {
                let raw = (lo as u16) | ((hi as u16) << 8);
                if (300..9000).contains(&raw) {
                    Some(raw)
                } else {
                    None
                }
            }
            _ => None,
        };

        (f1, f2)
    }

    /// Set fan boost register (0xEC)
    #[allow(dead_code)]
    pub fn set_fan_boost(&self, on: bool) {
        let _ = self.write_reg(REG_FAN_BOOST, if on { 0x0C } else { 0x00 });
    }

    /// Set manual fan percentage on both fans and refresh countdown timer
    #[allow(dead_code)]
    pub fn hold_fans_pct(&self, pct1: u8, pct2: u8) -> Result<(), ()> {
        let p1 = pct1.clamp(0, 100);
        let p2 = pct2.clamp(0, 100);

        let _ = self.write_reg(REG_MANUAL, MANUAL_ON);
        let _ = self.write_reg(REG_FAN_SET1_PCT, p1);
        let _ = self.write_reg(REG_FAN_SET2_PCT, p2);

        let srp1 = ((p1 as u32 * 58) / 100) as u8;
        let srp2 = ((p2 as u32 * 58) / 100) as u8;
        let _ = self.write_reg(REG_FAN_SET1_RPM100, srp1);
        let _ = self.write_reg(REG_FAN_SET2_RPM100, srp2);

        let _ = self.write_reg(REG_COUNTDOWN, COUNTDOWN_HOLD);
        Ok(())
    }

    /// Hand fans back to automatic EC firmware control
    #[allow(dead_code)]
    pub fn release_fans_to_auto(&self) -> Result<(), ()> {
        let _ = self.write_reg(REG_FAN_SET1_PCT, 0);
        let _ = self.write_reg(REG_FAN_SET2_PCT, 0);
        let _ = self.write_reg(REG_FAN_SET1_RPM100, 0);
        let _ = self.write_reg(REG_FAN_SET2_RPM100, 0);
        let _ = self.write_reg(REG_COUNTDOWN, 0);
        let _ = self.write_reg(REG_FAN_BOOST, 0);
        self.write_reg(REG_MANUAL, MANUAL_OFF)
    }
}
