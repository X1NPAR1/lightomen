#!/usr/bin/env bash
set -eo pipefail

echo "=========================================================="
echo "  LightOmen Linux: Complete Native System Installer"
echo "  Hardware Control for HP Omen & Victus Laptops"
echo "=========================================================="

if [ "$EUID" -ne 0 ]; then
    echo "ERROR: Please run as root or using sudo: sudo ./install.sh"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAL_USER="${SUDO_USER:-$USER}"
USER_HOME=$(getent passwd "$REAL_USER" | cut -d: -f6)

# 1. Detect Desktop Session (Wayland vs X11)
echo -e "\n[1/7] Detecting Display Session..."
WAYLAND_DETECTED=0
if [ -n "$WAYLAND_DISPLAY" ] || [ -e "/run/user/$(id -u "$REAL_USER")/wayland-0" ]; then
    WAYLAND_DETECTED=1
    echo "  -> Native Wayland session detected for user: $REAL_USER"
else
    echo "  -> X11 / XWayland fallback session detected for user: $REAL_USER"
fi

# 2. Install Kernel & Build Dependencies
echo -e "\n[2/7] Installing Kernel & System Dependencies..."
if command -v apt-get &>/dev/null; then
    apt-get update -qq
    apt-get install -y -qq dkms build-essential linux-headers-"$(uname -r)" \
        libudev-dev libfontconfig1-dev libx11-dev libdbus-1-dev
elif command -v pacman &>/dev/null; then
    pacman -Sy --noconfirm --needed dkms base-devel linux-headers dbus
elif command -v dnf &>/dev/null; then
    dnf install -y -q dkms kernel-devel kernel-headers dbus-devel
fi

# 3. Adapt & Install hp-omen-extra DKMS Driver
echo -e "\n[3/7] Installing Patched hp-omen-extra DKMS Driver..."
DKMS_SRC="/usr/src/hp-omen-extra-2.2.0"
rm -rf "$DKMS_SRC"
mkdir -p "$DKMS_SRC"
cp -r "$SCRIPT_DIR/driver/"* "$DKMS_SRC/"

dkms remove -m hp-omen-extra -v 2.2.0 --all 2>/dev/null || true
dkms add -m hp-omen-extra -v 2.2.0
dkms build -m hp-omen-extra -v 2.2.0
dkms install -m hp-omen-extra -v 2.2.0 --force

# Modprobe override configuration
cat << 'EOF' > /etc/modprobe.d/lightomen-hpwmi-override.conf
# Prioritize patched hp-wmi and hp-omen-extra from DKMS updates
blacklist hp_wmi
install hp_wmi /sbin/modprobe --ignore-install hp_wmi $CMDLINE_OPTS
EOF

# Reload modules if possible
modprobe -r hp_omen_extra 2>/dev/null || true
modprobe hp_omen_extra 2>/dev/null || true

# 4. Stop any existing running services/processes before binary installation
echo -e "\n[4/7] Preparing system & stopping existing services..."
systemctl stop lightomen-daemon.service 2>/dev/null || true
systemctl stop ohman-daemon.service 2>/dev/null || true
systemctl disable ohman-daemon.service 2>/dev/null || true
pkill -9 -x lightomen-gui 2>/dev/null || true
pkill -9 -x ohman-gui 2>/dev/null || true
pkill -9 -x lightomen-tray 2>/dev/null || true
pkill -9 -x ohman-tray 2>/dev/null || true
rm -rf /run/lightomen /run/ohman
rm -f /usr/local/bin/ohman-daemon /usr/local/bin/ohman-gui /usr/local/bin/ohman-tray
sleep 1

# 5. Check for Pre-compiled Binaries or Build from Source
echo -e "\n[5/7] Preparing Binaries..."
DAEMON_SRC=""
GUI_SRC=""
TRAY_SRC=""

if command -v cargo &>/dev/null; then
    echo "  -> Compiling fresh release binaries from source via Cargo (lto=fat, opt-level=z)..."
    sudo -u "$REAL_USER" cargo build --release --workspace --manifest-path "$SCRIPT_DIR/Cargo.toml"
    DAEMON_SRC="$SCRIPT_DIR/target/release/lightomen-daemon"
    GUI_SRC="$SCRIPT_DIR/target/release/lightomen-gui"
    TRAY_SRC="$SCRIPT_DIR/target/release/lightomen-tray"
elif [ -f "$SCRIPT_DIR/bin/lightomen-daemon" ] && [ -f "$SCRIPT_DIR/bin/lightomen-gui" ] && [ -f "$SCRIPT_DIR/bin/lightomen-tray" ]; then
    echo "  -> Found pre-compiled release binaries in ./bin/ (Skipping Cargo compilation)"
    DAEMON_SRC="$SCRIPT_DIR/bin/lightomen-daemon"
    GUI_SRC="$SCRIPT_DIR/bin/lightomen-gui"
    TRAY_SRC="$SCRIPT_DIR/bin/lightomen-tray"
elif [ -f "$SCRIPT_DIR/target/release/lightomen-daemon" ] && [ -f "$SCRIPT_DIR/target/release/lightomen-gui" ] && [ -f "$SCRIPT_DIR/target/release/lightomen-tray" ]; then
    echo "  -> Found existing built release binaries in ./target/release/"
    DAEMON_SRC="$SCRIPT_DIR/target/release/lightomen-daemon"
    GUI_SRC="$SCRIPT_DIR/target/release/lightomen-gui"
    TRAY_SRC="$SCRIPT_DIR/target/release/lightomen-tray"
else
    echo "ERROR: 'cargo' is not installed and pre-compiled binaries were not found."
    echo "Please install Rust (https://rustup.rs) or download the pre-compiled release package from:"
    echo "  https://github.com/zyr1on/lightomen/releases"
    exit 1
fi

# 6. Install Binaries, Symlinks, Icons & Desktop Files
echo -e "\n[6/7] Installing Executables, Icons & Desktop Integration..."
install -m 755 "$DAEMON_SRC" /usr/local/bin/lightomen-daemon
install -m 755 "$GUI_SRC" /usr/local/bin/lightomen-gui
install -m 755 "$TRAY_SRC" /usr/local/bin/lightomen-tray

# Backward compatibility symlinks
ln -sf /usr/local/bin/lightomen-daemon /usr/local/bin/ohman-daemon
ln -sf /usr/local/bin/lightomen-gui /usr/local/bin/ohman-gui
ln -sf /usr/local/bin/lightomen-tray /usr/local/bin/ohman-tray

# Install High-Resolution Icons
mkdir -p /usr/share/icons/hicolor/512x512/apps /usr/share/pixmaps
if [ -f "$SCRIPT_DIR/assets/lightomen.png" ]; then
    install -m 644 "$SCRIPT_DIR/assets/lightomen.png" /usr/share/icons/hicolor/512x512/apps/lightomen.png
    install -m 644 "$SCRIPT_DIR/assets/lightomen.png" /usr/share/pixmaps/lightomen.png
    ln -sf /usr/share/icons/hicolor/512x512/apps/lightomen.png /usr/share/icons/hicolor/512x512/apps/ohman.png
    ln -sf /usr/share/pixmaps/lightomen.png /usr/share/pixmaps/ohman.png
    if command -v gtk-update-icon-cache &>/dev/null; then
        gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
    fi
fi

# Desktop Entry
mkdir -p /usr/share/applications
cat << 'EOF' > /usr/share/applications/lightomen.desktop
[Desktop Entry]
Name=LightOmen
GenericName=HP Omen & Victus Controller
Comment=Hardware Performance, Fan Curve & MUX Switch Controller
Exec=env DRI_PRIME=0 __NV_PRIME_RENDER_OFFLOAD=0 /usr/local/bin/lightomen-gui
Icon=lightomen
Terminal=false
Type=Application
StartupWMClass=lightomen
Categories=Settings;HardwareSettings;System;
Keywords=omen;victus;hp;fan;cooling;curve;performance;mux;
EOF
ln -sf /usr/share/applications/lightomen.desktop /usr/share/applications/ohman.desktop

# Autostart for tray
if [ -n "$USER_HOME" ]; then
    mkdir -p "$USER_HOME/.config/autostart"
    cat << 'EOF' > "$USER_HOME/.config/autostart/lightomen-tray.desktop"
[Desktop Entry]
Type=Application
Name=LightOmen Tray
Comment=LightOmen Hardware Tray Controller
Exec=env DRI_PRIME=0 __NV_PRIME_RENDER_OFFLOAD=0 /usr/local/bin/lightomen-tray
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
EOF
    rm -f "$USER_HOME/.config/autostart/ohman-gui.desktop" 2>/dev/null || true
    rm -f "$USER_HOME/.config/autostart/lightomen-gui.desktop" 2>/dev/null || true
    chown -R "$REAL_USER:$REAL_USER" "$USER_HOME/.config/autostart"
fi

# Config directory setup
mkdir -p /etc/lightomen /etc/ohman
chmod 755 /etc/lightomen /etc/ohman

# Systemd Service Installation
install -m 644 "$SCRIPT_DIR/systemd/lightomen-daemon.service" /etc/systemd/system/lightomen-daemon.service
ln -sf /etc/systemd/system/lightomen-daemon.service /etc/systemd/system/ohman-daemon.service 2>/dev/null || true

systemctl daemon-reload
systemctl enable lightomen-daemon.service
systemctl restart lightomen-daemon.service

# Start System Tray companion immediately for active user session
if [ -n "$REAL_USER" ] && [ "$REAL_USER" != "root" ]; then
    REAL_UID=$(id -u "$REAL_USER")
    sudo -u "$REAL_USER" XDG_RUNTIME_DIR="/run/user/$REAL_UID" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$REAL_UID/bus" systemd-run --user --unit=lightomen-tray --collect /usr/local/bin/lightomen-tray 2>/dev/null || true
    echo "  -> lightomen-tray successfully spawned in system tray for $REAL_USER."
fi

# 7. Final Status & Verification Check
echo -e "\n[7/7] Verifying Daemon Status & Safety Reset..."
sleep 1.5
if systemctl is-active --quiet lightomen-daemon.service; then
    echo "  -> lightomen-daemon.service is ACTIVE and RUNNING."
else
    echo "  -> Warning: lightomen-daemon.service status check returned non-zero."
fi

# Ensure fans are unlatched and resting at Auto quiet speed
if [ -e "/sys/devices/platform/hp-wmi/hwmon" ]; then
    HWMON_DIR=$(find /sys/devices/platform/hp-wmi/hwmon -maxdepth 1 -name "hwmon*" | head -n 1)
    if [ -n "$HWMON_DIR" ] && [ -w "$HWMON_DIR/pwm1_enable" ]; then
        echo 2 > "$HWMON_DIR/pwm1_enable" 2>/dev/null || true
        echo "  -> Motherboard cooling verified: BIOS Auto control active."
    fi
fi

echo "=========================================================="
echo "  LightOmen Linux INSTALLATION COMPLETE!"
echo "  Launch GUI via terminal:  lightomen-gui (or ohman-gui)"
echo "  Or click System Tray / press physical OMEN hotkey."
echo "=========================================================="
