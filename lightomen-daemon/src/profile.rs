// profile.rs: Power Profiles (Eco, Balanced, Performance) & MUX Switch
// Controls platform_profile, GPU TGP boost, PPAB, and gpu_mux_mode.

use crate::hwmon::HwmonBackend;
use lightomen_common::{
    MUX_MODE_DISCRETE, MUX_MODE_HYBRID, POWER_PROFILE_BALANCED, POWER_PROFILE_ECO,
    POWER_PROFILE_PERFORMANCE,
};

pub struct ProfileController;

impl ProfileController {
    pub fn apply_power_profile(hwmon: &mut HwmonBackend, profile: u8) {
        match profile {
            POWER_PROFILE_ECO => {
                // Low power profile, no GPU wattage boost
                let _ = hwmon.set_platform_profile("low-power\n");
                let _ = hwmon.set_platform_profile("quiet\n");
                let _ = hwmon.set_gpu_tgp(false);
                let _ = hwmon.set_gpu_ppab(false);
            }
            POWER_PROFILE_BALANCED => {
                // Balanced profile with standard PPAB
                let _ = hwmon.set_platform_profile("balanced\n");
                let _ = hwmon.set_gpu_tgp(false);
                let _ = hwmon.set_gpu_ppab(true);
            }
            POWER_PROFILE_PERFORMANCE => {
                // Performance profile with maximum GPU TGP and PPAB boost
                let _ = hwmon.set_platform_profile("performance\n");
                let _ = hwmon.set_gpu_tgp(true);
                let _ = hwmon.set_gpu_ppab(true);
            }
            _ => {}
        }
    }

    pub fn apply_mux_mode(hwmon: &mut HwmonBackend, mode: u8) {
        match mode {
            MUX_MODE_HYBRID => {
                let _ = hwmon.set_mux_mode(0);
            }
            MUX_MODE_DISCRETE => {
                let _ = hwmon.set_mux_mode(1);
            }
            _ => {}
        }
    }
}
