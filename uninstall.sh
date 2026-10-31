#!/usr/bin/env bash
set -eo pipefail

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"

echo "=========================================================="
echo "  LightOmen Linux: Complete Native System Uninstaller"
echo "  Clean Removal of Daemons, Sockets, Drivers & Residues"
echo "=========================================================="

if [ "$EUID" -ne 0 ]; then
    echo "ERROR: Please run as root or using sudo: sudo ./uninstall.sh"
    exit 1
fi

REAL_USER="${SUDO_USER:-$USER}"
USER_HOME=$(getent passwd "$REAL_USER" | cut -d: -f6)

# 1. Stop & Disable Systemd Services
echo -e "\n[1/7] Stopping and Disabling Systemd Services..."
for s in lightomen-daemon.service ohman-daemon.service; do
    if systemctl is-active --quiet "$s" 2>/dev/null; then
        DAEMON_PID=$(systemctl show -p MainPID --value "$s" 2>/dev/null || true)
        echo "  -> Stopping $s (PID: $DAEMON_PID)..."
        systemctl stop "$s" 2>/dev/null || true
    fi
    systemctl disable "$s" 2>/dev/null || true
    rm -f "/etc/systemd/system/$s" "/etc/systemd/system/multi-user.target.wants/$s"
done
systemctl daemon-reload
echo "  [OK] Systemd services stopped and removed."

# 2. Terminate Running Processes with PID Verification
echo -e "\n[2/7] Terminating Running Instances..."
TARGET_PROCS=("lightomen-daemon" "ohman-daemon" "lightomen-gui" "ohman-gui" "lightomen-tray" "ohman-tray")
for proc in "${TARGET_PROCS[@]}"; do
    pids=$(pidof "$proc" 2>/dev/null || pgrep -f "/usr/local/bin/$proc" 2>/dev/null || true)
    if [ -n "$pids" ]; then
        echo "  -> Found active process: $proc (PID: $pids), terminating..."
        kill -9 $pids 2>/dev/null || true
        sleep 0.2
        check_pids=$(pidof "$proc" 2>/dev/null || pgrep -f "/usr/local/bin/$proc" 2>/dev/null || true)
        if [ -z "$check_pids" ]; then
            echo "     [OK] Terminated $proc (PID: $pids successfully exited)"
        else
            echo "     [WARNING] Process $proc still alive (PID: $check_pids)"
        fi
    else
        echo "  -> $proc: No active instances running (Clean)"
    fi
done

# 3. Hardware Safety Reset: Restore Fans to BIOS Auto Control
echo -e "\n[3/7] Restoring Hardware Cooling to Safe BIOS Auto Control..."
RESTORED=0
for h in /sys/class/hwmon/hwmon*; do
    if [ -f "$h/name" ] && [ "$(cat "$h/name" 2>/dev/null)" = "hp" ]; then
        if [ -w "$h/pwm1_enable" ]; then
            echo 2 > "$h/pwm1_enable" 2>/dev/null || true
            echo "  [OK] pwm1_enable set to 2 (BIOS Auto Control active)."
            RESTORED=1
        fi
        break
    fi
done
if [ "$RESTORED" -eq 0 ] && [ -e "/sys/devices/platform/hp-wmi/hwmon" ]; then
    HWMON_DIR=$(find /sys/devices/platform/hp-wmi/hwmon -maxdepth 1 -name "hwmon*" | head -n 1)
    if [ -n "$HWMON_DIR" ] && [ -w "$HWMON_DIR/pwm1_enable" ]; then
        echo 2 > "$HWMON_DIR/pwm1_enable" 2>/dev/null || true
        echo "  [OK] pwm1_enable set to 2 (BIOS Auto Control active)."
    fi
fi

# 4. Remove DKMS Driver & Source
echo -e "\n[4/7] Removing hp-omen-extra DKMS Driver..."
if command -v dkms &>/dev/null; then
    for v in $(dkms status hp-omen-extra 2>/dev/null | grep -oP "(?<=hp-omen-extra[/, ])[^,:]+" | tr -d ' ' | sort -u); do
        [ -n "$v" ] && dkms remove -m hp-omen-extra -v "$v" --all 2>/dev/null || true
    done
fi
dkms remove -m hp-omen-extra -v 2.2.0 --all 2>/dev/null || true
rm -rf /usr/src/hp-omen-extra-*

# Purge any custom .ko files in updates/ so the stock kernel module takes over
KVER=$(uname -r)
find /lib/modules/"$KVER" /usr/lib/modules/"$KVER" -name 'hp-omen-extra.ko*' -delete 2>/dev/null || true
find /lib/modules/"$KVER" /usr/lib/modules/"$KVER" -path '*/updates/*' -name 'hp-wmi.ko*' -delete 2>/dev/null || true
depmod -a

# Restore stock kernel modules
echo "  • Unloading custom DKMS modules and restoring stock kernel driver..."
modprobe -r hp_omen_extra 2>/dev/null || true
modprobe -r hp_wmi 2>/dev/null || true
modprobe hp_wmi 2>/dev/null || true
echo "  [OK] Custom DKMS modules removed and stock hp_wmi restored."

# 5. Clean Up Kernel Autoload & Modprobe Configs
echo -e "\n[5/7] Cleaning Up Kernel Modules Autoload & Modprobe Configs..."
for cfg in /etc/modules-load.d/hp-omen-extra.conf \
           /etc/modules-load.d/lightomen.conf; do
    if [ -f "$cfg" ]; then
        echo "  -> Removed: $cfg"
        rm -f "$cfg"
    fi
done

# Remove any modprobe blacklists or override files
for f in /etc/modprobe.d/*hpwmi*.conf /etc/modprobe.d/*hp-wmi*.conf /etc/modprobe.d/*hp_wmi*.conf \
         /etc/modprobe.d/*lightomen*.conf /etc/modprobe.d/*ohman*.conf /etc/modprobe.d/*hp-omen*.conf /etc/modprobe.d/*omen-extra*.conf; do
    if [ -f "$f" ]; then
        echo "  -> Removed modprobe blacklist config: $f"
        rm -f "$f"
    fi
done
echo "  [OK] Autoload & Modprobe configs cleaned."

# 6. Remove Installed Binaries, Desktop Files & Icons
echo -e "\n[6/7] Removing Executables, Icons & Desktop Integrations..."
for bin in /usr/local/bin/lightomen-daemon \
           /usr/local/bin/lightomen-gui \
           /usr/local/bin/lightomen-tray \
           /usr/local/bin/lightomen-uninstall \
           /usr/local/bin/ohman-daemon \
           /usr/local/bin/ohman-gui \
           /usr/local/bin/ohman-tray \
           /usr/local/bin/ohman-uninstall; do
    if [ -e "$bin" ]; then
        echo "  -> Removed binary: $bin"
        rm -f "$bin"
    fi
done

for d in /usr/share/applications/lightomen.desktop /usr/share/applications/ohman.desktop; do
    if [ -f "$d" ]; then
        echo "  -> Removed desktop file: $d"
        rm -f "$d"
    fi
done

for icon in /usr/share/icons/hicolor/512x512/apps/lightomen.png \
            /usr/share/icons/hicolor/512x512/apps/ohman.png \
            /usr/share/pixmaps/lightomen.png \
            /usr/share/pixmaps/ohman.png; do
    if [ -f "$icon" ]; then
        echo "  -> Removed icon: $icon"
        rm -f "$icon"
    fi
done

if command -v gtk-update-icon-cache &>/dev/null; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi

# Clean user autostart
if [ -n "$USER_HOME" ]; then
    for a in "$USER_HOME/.config/autostart/lightomen-tray.desktop" \
             "$USER_HOME/.config/autostart/lightomen-gui.desktop" \
             "$USER_HOME/.config/autostart/ohman-gui.desktop" \
             "$USER_HOME/.config/autostart/ohman-tray.desktop"; do
        if [ -f "$a" ]; then
            echo "  -> Removed autostart entry: $a"
            rm -f "$a"
        fi
    done
fi
echo "  [OK] Executables, icons and desktop files cleaned."

# 7. Remove Runtime & Configuration Directories
echo -e "\n[7/7] Removing Runtime & Configuration Directories..."
for r_dir in /run/lightomen /run/ohman; do
    if [ -e "$r_dir" ]; then
        echo "  -> Removed runtime directory: $r_dir"
        rm -rf "$r_dir"
    fi
done

for c_dir in /etc/lightomen /etc/ohman; do
    if [ -e "$c_dir" ]; then
        echo "  -> Removed configuration directory: $c_dir"
        rm -rf "$c_dir"
    fi
done
echo "  [OK] Runtime and configuration directories removed."

echo -e "\n=========================================================="
echo "  LightOmen Linux UNINSTALLATION COMPLETE!"
echo "  The system has been restored to factory clean state."
echo "  To reinstall at any time: sudo ./install.sh"
echo "=========================================================="
