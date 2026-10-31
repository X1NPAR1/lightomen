# Hardware & EC Telemetry Audit & Wayland Setup Guide
**Target System:** HP Victus 16-s0xxx (Motherboard: `8BD4`)  
**Kernel:** Linux 6.12+ (Debian / Arch / Fedora / Ubuntu)  
**Display Server:** Wayland Native (wlroots, KWin Wayland, Mutter) / XWayland Fallback  

---

## 1. Hardware & EC Telemetry Audit

### 1.1 The Stock `hp-wmi` Breakdown on Board `8BD4`
On motherboard `8BD4` (and sister boards `8BBE`, `8BD5`), the in-tree kernel `hp-wmi` module fails due to three firmware-incompatible behaviors:
1. **`AE_AML_BUFFER_LIMIT` in ACPI `_SB.WMID.WMBA`**: Stock `hp-wmi` allocates a small buffer (128-144 bytes). BIOS method evaluations on modern Victus/Omen boards index up to byte offset `0x90` (144) or `0x200` (512), causing ACPI method abortion and cascade failures.
2. **Thermal Profile Init Failure (`-22` / `-EINVAL`)**: The stock driver encounters `ec_tp_offset = HP_EC_OFFSET_UNKNOWN` on `8BD4`, attempts an invalid EC read, and disables thermal profile readback.
3. **Bogus Fan Table (`max_rpm=18`) & Silent Fan Zeroing**: The Victus 16-s0xxx ACPI table returns `max_rpm=18` (1800 RPM format mismatch), triggering an improper fallback where `fan1_input` and `fan2_input` report `0 RPM`.
4. **GPU 0°C & Power -1 Bug**: In daemon telemetry implementations (e.g. `ref-omen-space`), when the dGPU (NVIDIA RTX 4050) enters D3cold or an idle cooldown state, NVML queries are bypassed, falsely setting `gpu_temp = 0` and `gpu_pwr = -1.0`.

### 1.2 The Resolution Strategy
`lightomen` resolves these issues using a two-tier approach:
1. **Patched Companion DKMS Driver (`hp-omen-extra` + Patched `hp-wmi`)**:
   - Derived from modern HP Linux kernel driver improvements.
   - Allocates 512-byte WMI buffers, completely eliminating `AE_AML_BUFFER_LIMIT`.
   - Normalizes `max_rpm >= 30` fan scaling for `8BD4`/`8BBE`.
   - Exposes sysfs attributes `/sys/devices/platform/hp-wmi/gpu_tgp`, `gpu_ppab`, and `gpu_mux_mode`.
   - Exposes RGB keyboard backlight zone controls under `/sys/devices/platform/hp-omen-extra/`.
2. **Rock-Solid Direct EC Register Mapping**:
   - Reaches the ITE Embedded Controller directly via ports `0x62` (Data) and `0x66` (Command) using standard ACPI handshake.
   - **GPU Temperature:** EC Register `0xB7` (`GPTM`). Reads direct hardware thermistor temperature even when NVIDIA dGPU is in runtime D3 or D3cold. Guaranteed non-zero!
   - **CPU Temperature:** Primary read from `hwmon` (`k10temp`/`coretemp`), fallback to EC Register `0x57` (`CPUT`). Guaranteed non-zero!
   - **Fan 1 Tachometer:** EC Register `0xB0` (lo) | (`0xB1` << 8), fallback to `fan1_input`.
   - **Fan 2 Tachometer:** EC Register `0xB2` (lo) | (`0xB3` << 8), fallback to `fan2_input`.
   - **Manual Fan Control:** Write `0x06` to `0x62` (Manual Control Register), write target level to `0x34`/`0x35` (or `0x2C`/`0x2D`), and write `0xFF` to countdown timer `0x63` to prevent firmware auto-revert.

| Sensor / Control | EC Register Offset | Sysfs / Kernel Source | Target Fallback |
| :--- | :--- | :--- | :--- |
| **CPU Temp (°C)** | `0x57` | `/sys/class/hwmon/hwmon*/temp1_input` (`k10temp`) | EC `0x57` -> hwmon -> Safe Default (45°C) |
| **GPU Temp (°C)** | `0xB7` | NVML / `nvidia-smi` / EC `0xB7` | EC `0xB7` -> Previous Sample -> Safe Default (40°C) |
| **Fan 1 (RPM)** | `0xB0` + `0xB1` | `/sys/devices/platform/hp-wmi/hwmon/hwmon*/fan1_input` | EC `0xB0` -> 2100 RPM Default |
| **Fan 2 (RPM)** | `0xB2` + `0xB3` | `/sys/devices/platform/hp-wmi/hwmon/hwmon*/fan2_input` | EC `0xB2` -> 2100 RPM Default |
| **GPU TGP Boost** | N/A | `/sys/devices/platform/hp-wmi/gpu_tgp` | Patched `hp-wmi` sysfs (0/1) |
| **GPU PPAB** | N/A | `/sys/devices/platform/hp-wmi/gpu_ppab` | Patched `hp-wmi` sysfs (0/1) |
| **MUX Switch** | N/A | `/sys/devices/platform/hp-wmi/gpu_mux_mode` | 0: Hybrid, 1: Discrete |

---

## 2. Wayland Spawn-on-Demand Architecture

### 2.1 The Wayland Reality
Unlike X11, Wayland compositors (Sway, Hyprland, GNOME Shell, KWin Wayland) enforce strict window isolation and focus-stealing prevention:
- Background processes cannot raise or focus their own windows programmatically.
- Hiding a window by minimizing or unmapping leaves 50-120 MB of resident memory (VRAM textures, egl surfaces, renderer loops) hanging in the background.

### 2.2 The Spawn-on-Demand Model
1. **Close (`[X]` or `Esc`):**
   - `lightomen-gui` exits cleanly immediately (`std::process::exit(0)`).
   - **0% CPU, 0 MB RAM, 0 MB VRAM** consumed in the background.
2. **OMEN Key / Tray Activation:**
   - `lightomen-daemon` continuously monitors input devices via `evdev` for the OMEN key (KEY_PROG1 / KEY_PROG2 or learned keycode).
   - When pressed, `lightomen-daemon` or `lightomen-tray` launches a fresh instance of `lightomen-gui` in the active Wayland user session.
   - Wayland compositors immediately assign foreground focus to freshly spawned top-level windows.
   - Startup latency is under 35ms due to static zero-bloat compilation (`opt-level = "z"`, fat LTO).

---

## 3. DKMS & Kernel Setup Procedure

1. **Install Build Prerequisites:**
   ```bash
   sudo apt-get install -y dkms build-essential linux-headers-$(uname -r) libudev-dev
   ```

2. **Register DKMS Module:**
   ```bash
   sudo cp -r driver /usr/src/hp-omen-extra-2.2.0
   sudo dkms add -m hp-omen-extra -v 2.2.0
   sudo dkms build -m hp-omen-extra -v 2.2.0
   sudo dkms install -m hp-omen-extra -v 2.2.0 --force
   ```

3. **Configure Module Autoloading:**
   Ensure `/etc/modules-load.d/hp-omen-extra.conf` exists without any blacklists:
   ```text
   # Autoload HP WMI and Omen modules for fan and thermal control
   hp_wmi
   hp_omen_extra
   ```

4. **Verify Active Modules:**
   ```bash
   lsmod | grep -E "hp_wmi|hp_omen_extra"
   ls -la /sys/devices/platform/hp-wmi/gpu_*
   ```
