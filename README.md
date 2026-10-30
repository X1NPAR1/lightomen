# LightOmen

**Ultra-Lightweight Hardware Control Center for HP OMEN & Victus Laptops on Linux**

Written in 100% pure, idiomatic Rust with zero runtime bloat — no Tokio, no Serde, pure standard library, `libc`, and `evdev`. Consumes under 2.5 MB RAM in the background.

![LightOmen UI](assets/lightomen.png)

---

## Key Features

* **Dynamic Fan Curve Editor (with True 0 RPM Silent Mode)**:
  * 7-point interactive curve editor for CPU and GPU.
  * **Fan-Stop (0 RPM)**: Drag points to the bottom to completely stop the fans for whisper-quiet idle operation.
  * **Hardware Operational Floor (~1800 RPM)**: Automatically engages the fan safely when temperature thresholds are crossed.
  * **Link Curves**: Synchronize CPU and GPU fan curves with one click or tune them independently.
  * Quick Presets: Quiet, Balanced, and Aggressive.

* **Power & Performance Profiles**:
  * **Eco**: Power-saving mode with reduced fan activity and cool temps.
  * **Balanced**: Daily driver mode balancing cooling and battery life.
  * **Performance**: Unlocks maximum power limits, enabling GPU TGP boost and dynamic PPAB wattage.

* **Hardware MUX Switch**:
  * Easily toggle between **Hybrid (AMD iGPU + NVIDIA PRIME)** for battery efficiency and **Discrete (Direct dGPU)** for maximum gaming FPS and lowest display latency.

* **Real-Time Hardware Telemetry**:
  * Live CPU and GPU temperatures with EMA smoothing (eliminates AMD Precision Boost fan cycling).
  * True hardware readings even when NVIDIA dGPU enters `D3cold` power-saving states.
  * Live tachometer fan RPM monitoring and wattage power consumption.
  * Auto-detection of motherboard ID (e.g., `8BD4`).

* **Physical OMEN Hotkey & Key Learning**:
  * Press your laptop's physical OMEN key to instantly open or close the control panel.
  * Built-in interactive hotkey learning mode to pair any custom key code.

* **Wayland-Native Spawn-on-Demand (Zero Background Bloat)**:
  * When closed, the graphical interface exits completely: **0 MB RAM, 0% CPU**.
  * Spawns instantly in under 35ms when triggered from the OMEN key or system tray.
  * Native Wayland window dragging via left-click or right-click on the top header.

* **Lightweight System Tray Companion**:
  * Real-time temperature display in your desktop tray (KDE Plasma, GNOME, Sway, Hyprland).
  * Quick profile and fan mode switching without opening the main window.

---

## Installation

### 1. Clone the repository
```bash
git clone https://github.com/zyr1on/lightomen.git
cd lightomen
```

### 2. Run the automated installer
```bash
chmod +x install.sh
sudo ./install.sh
```

The installer will:
1. Detect your session (Wayland / X11).
2. Install required kernel headers and build dependencies.
3. Install the patched `hp-omen-extra` DKMS kernel module for complete WMI support.
4. Compile release binaries with link-time optimization (`lto = "fat"`, `opt-level = "z"`).
5. Install systemd services, desktop entries, and tray autostart.

---

## Usage

* **Open GUI via Terminal:**
  ```bash
  lightomen-gui
  ```
* **Toggle with OMEN Hotkey:** Press the physical OMEN key on your keyboard.
* **System Tray:** Click the LightOmen icon in your system tray to open the UI or right-click to quickly change profiles.
* **Service Management:**
  ```bash
  sudo systemctl status lightomen-daemon.service
  ```

---

## Project Architecture

LightOmen is organized as a lightweight Rust workspace:

```text
lightomen/
├── lightomen-common/    # #[repr(C)] binary IPC protocol & configuration structures
├── lightomen-daemon/    # Low-level root service (sysfs, EC I/O, evdev hotkeys, IPC socket)
├── lightomen-gui/       # Wayland-native eframe/egui interface & system tray companion
├── driver/              # Patched hp-omen-extra DKMS kernel module source
├── assets/              # Icons and application branding
├── systemd/             # Systemd service units
└── install.sh           # One-click native installer script
```

### Memory Footprint Comparison

| Component | Proprietary Software | LightOmen |
| :--- | :--- | :--- |
| Background Daemon | ~150 - 300 MB | ~2.3 MB |
| Idle Tray Companion | ~80 - 150 MB | ~3.4 MB |
| GUI (Closed) | ~100 MB | Strictly 0 MB |

---

## Testing

Run automated workspace unit tests and end-to-end integration tests:

```bash
cargo test --workspace
sudo ./test-e2e.sh
```

---

## License

This project is licensed under the MIT License / GPL v2.
