// fan.rs: Fan Controller (Auto, Max, Curve)
// 7-point independent curve calculation, keep-alive loop, and >=90°C emergency max.

use crate::ec::EmbeddedController;
use crate::hwmon::HwmonBackend;
use lightomen_common::{CURVE_TEMPS, FAN_MODE_AUTO, FAN_MODE_CURVE, FAN_MODE_MAX};

pub struct FanController;

impl FanController {
    /// Piecewise linear interpolation for a 7-point fan curve
    pub fn evaluate_curve(curve: &[u8; 7], temp: u16) -> u8 {
        if temp <= CURVE_TEMPS[0] as u16 {
            return curve[0];
        }
        if temp >= CURVE_TEMPS[6] as u16 {
            return curve[6].max(100);
        }

        for i in 0..6 {
            let t0 = CURVE_TEMPS[i] as u16;
            let t1 = CURVE_TEMPS[i + 1] as u16;
            if temp >= t0 && temp <= t1 {
                let p0 = curve[i] as f32;
                let p1 = curve[i + 1] as f32;
                let fraction = (temp - t0) as f32 / (t1 - t0) as f32;
                // If starting from 0% (fan off / silent mode), stay at 0 RPM
                // until fraction crosses halfway (e.g. 30°C..34°C stays 0 RPM)
                if p0 == 0.0 && p1 > 0.0 && fraction < 0.5 {
                    return 0;
                }
                let result = p0 + fraction * (p1 - p0);
                return (result.round() as u8).clamp(0, 100);
            }
        }

        curve[6]
    }

    /// Convert fan percentage (0..100) to target RPM
    pub fn pct_to_rpm(pct: u8, max_rpm: u16) -> u16 {
        if pct == 0 {
            0
        } else {
            let rpm = (pct as u32 * max_rpm as u32 / 100) as u16;
            // Floor of functional spinning speed is ~1800 RPM
            rpm.clamp(1800, max_rpm)
        }
    }

    /// Apply fan mode and calculated speeds to hardware
    pub fn apply(
        _ec: &EmbeddedController,
        hwmon: &mut HwmonBackend,
        mode: u8,
        cpu_curve: &[u8; 7],
        gpu_curve: &[u8; 7],
        cpu_temp: u16,
        gpu_temp: u16,
    ) {
        let max_rpm = hwmon.max_rpm;

        // >=90°C EMERGENCY MAX OVERRIDE
        if cpu_temp >= 90 || gpu_temp >= 90 {
            hwmon.apply_fan_mode(FAN_MODE_MAX, max_rpm, max_rpm);
            return;
        }

        match mode {
            FAN_MODE_AUTO => {
                hwmon.apply_fan_mode(FAN_MODE_AUTO, 0, 0);
            }
            FAN_MODE_MAX => {
                hwmon.apply_fan_mode(FAN_MODE_MAX, max_rpm, max_rpm);
            }
            FAN_MODE_CURVE => {
                let cpu_pct = Self::evaluate_curve(cpu_curve, cpu_temp);
                let gpu_pct = if gpu_temp == 0 {
                    0
                } else {
                    Self::evaluate_curve(gpu_curve, gpu_temp)
                };

                let target1 = Self::pct_to_rpm(cpu_pct, max_rpm);
                let target2 = Self::pct_to_rpm(gpu_pct, max_rpm);

                hwmon.apply_fan_mode(FAN_MODE_CURVE, target1, target2);
            }
            _ => {
                hwmon.apply_fan_mode(FAN_MODE_AUTO, 0, 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_curve_endpoints() {
        let curve = [20, 30, 40, 50, 60, 70, 100];
        assert_eq!(FanController::evaluate_curve(&curve, 25), 20);
        assert_eq!(FanController::evaluate_curve(&curve, 30), 20);
        assert_eq!(FanController::evaluate_curve(&curve, 90), 100);
        assert_eq!(FanController::evaluate_curve(&curve, 95), 100);
    }

    #[test]
    fn test_evaluate_curve_midpoint() {
        let curve = [20, 30, 40, 50, 60, 70, 100];
        // 35°C is midway between 30 (20%) and 40 (30%) -> 25%
        assert_eq!(FanController::evaluate_curve(&curve, 35), 25);
    }

    #[test]
    fn test_evaluate_curve_zero_rpm() {
        let curve = [0, 20, 30, 40, 50, 60, 100];
        assert_eq!(FanController::evaluate_curve(&curve, 25), 0);
        assert_eq!(FanController::evaluate_curve(&curve, 30), 0);
        assert_eq!(FanController::evaluate_curve(&curve, 32), 0);
        assert_eq!(FanController::evaluate_curve(&curve, 34), 0);
        assert_eq!(FanController::evaluate_curve(&curve, 35), 10);
        assert_eq!(FanController::evaluate_curve(&curve, 40), 20);
    }

    #[test]
    fn test_pct_to_rpm() {
        let max = 5800;
        assert_eq!(FanController::pct_to_rpm(0, max), 0);
        assert_eq!(FanController::pct_to_rpm(100, max), 5800);
        assert_eq!(FanController::pct_to_rpm(50, max), 2900);
        // Minimum functional floor clamps < 1800 to 1800 when pct > 0
        assert_eq!(FanController::pct_to_rpm(10, max), 1800);
    }
}
