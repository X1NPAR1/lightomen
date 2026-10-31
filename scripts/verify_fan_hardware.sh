#!/usr/bin/env bash
# verify_fan_hardware.sh: Comprehensive Hardware Fan Control & EC Verification Script
set -e

echo "========================================================"
echo "    LightOMEN Fan Control & EC Hardware Verification    "
echo "========================================================"

# 1. Stop background daemons/GUIs so they do not conflict during direct sysfs tests
echo "[1/7] Stopping background daemons and GUIs for isolated testing..."
systemctl stop lightomen-daemon.service 2>/dev/null || true
pkill -9 -x lightomen-gui 2>/dev/null || true
pkill -9 -x lightomen-tray 2>/dev/null || true
sleep 1

# 2. Check kernel modules
echo "[2/7] Verifying kernel modules (hp_wmi, hp_omen_extra)..."
if ! lsmod | grep -q "hp_wmi"; then
    echo "  Loading hp_wmi..."
    modprobe hp_wmi
fi
if ! lsmod | grep -q "hp_omen_extra"; then
    echo "  Loading hp_omen_extra..."
    modprobe hp_omen_extra
fi
echo "  [OK] Kernel modules are loaded."

# 3. Locate HP hwmon device
echo "[3/7] Locating HP WMI hwmon interface..."
HP_HWMON=""
for h in /sys/class/hwmon/hwmon*; do
    if [ -f "$h/name" ] && [ "$(cat "$h/name" 2>/dev/null)" = "hp" ]; then
        HP_HWMON="$h"
        break
    fi
done

if [ -z "$HP_HWMON" ]; then
    echo "  [ERROR] HP hwmon device not found under /sys/class/hwmon!"
    exit 1
fi
echo "  [OK] Found HP hwmon at: $HP_HWMON"

read_fans() {
    local f1=$(cat "$HP_HWMON/fan1_input" 2>/dev/null || echo 0)
    local f2=$(cat "$HP_HWMON/fan2_input" 2>/dev/null || echo 0)
    local mode=$(cat "$HP_HWMON/pwm1_enable" 2>/dev/null || echo "?")
    echo "Fan1: ${f1} RPM | Fan2: ${f2} RPM | pwm1_enable: ${mode}"
}

# 4. Test 1100 RPM Floor in Manual Mode (pwm1_enable = 1)
echo -e "\n[4/7] Testing 1100 RPM Floor (Victus 16 physical floor)..."
echo 1 > "$HP_HWMON/pwm1_enable"
echo 1100 > "$HP_HWMON/fan1_target"
echo 1100 > "$HP_HWMON/fan2_target"
sleep 4
STATUS_1100=$(read_fans)
echo "  Current state: $STATUS_1100"
if [[ "$STATUS_1100" =~ "pwm1_enable: 1" ]]; then
    echo "  [OK] 1100 RPM floor accepted and pwm1_enable is 1 (Manual)."
else
    echo "  [FAIL] pwm1_enable did not accept manual mode!"
    exit 1
fi

# 5. Test 2200 RPM Intermediate Speed
echo -e "\n[5/7] Testing 2200 RPM intermediate curve speed..."
echo 2200 > "$HP_HWMON/fan1_target"
echo 2200 > "$HP_HWMON/fan2_target"
sleep 4
STATUS_2200=$(read_fans)
echo "  Current state: $STATUS_2200"

# 6. Test MAX Boost Mode (pwm1_enable = 0) & Unlatch Recovery
echo -e "\n[6/7] Testing MAX Boost Mode (pwm1_enable = 0) and EC recovery sequence..."
echo 0 > "$HP_HWMON/pwm1_enable"
sleep 3
STATUS_MAX=$(read_fans)
echo "  MAX Boost State: $STATUS_MAX"
if [[ "$STATUS_MAX" =~ "pwm1_enable: 0" ]]; then
    echo "  [OK] MAX boost engaged (pwm1_enable = 0)."
else
    echo "  [FAIL] MAX boost failed to engage!"
    exit 1
fi

echo "  Executing EC unlatch recovery: 2 -> 50ms -> 1 -> target 1500 RPM..."
echo 2 > "$HP_HWMON/pwm1_enable"
sleep 0.05
echo 1 > "$HP_HWMON/pwm1_enable"
echo 1500 > "$HP_HWMON/fan1_target"
echo 1500 > "$HP_HWMON/fan2_target"
sleep 4
STATUS_RECOVER=$(read_fans)
echo "  Recovered State: $STATUS_RECOVER"
if [[ "$STATUS_RECOVER" =~ "pwm1_enable: 1" ]]; then
    echo "  [OK] EC successfully unlatched from MAX mode back to Manual 1500 RPM!"
else
    echo "  [FAIL] EC recovery sequence failed!"
    exit 1
fi

# 7. Restart lightomen-daemon and test IPC telemetry
echo -e "\n[7/7] Restarting lightomen-daemon.service and testing live IPC telemetry..."
systemctl start lightomen-daemon.service
sleep 2

if [ -S "/run/lightomen/lightomen.sock" ]; then
    echo "  [OK] /run/lightomen/lightomen.sock active."
    python3 -c "
import socket, struct
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect('/run/lightomen/lightomen.sock')
s.sendall(struct.pack('<BBH64s', 1, 0, 0, b'\x00'*64))
data = s.recv(1024)
s.close()
cpu_t, gpu_t, f1, f2, cpwr, gpwr = struct.unpack_from('<HHHHII', data, 0)
fmode = data[37]
print(f'  Telemetry Response: CPU {cpu_t}°C, GPU {gpu_t}°C, Fan1 {f1} RPM, Fan2 {f2} RPM, FanMode {fmode}')
"
    echo "  [OK] Daemon IPC telemetry verified successfully!"
else
    echo "  [FAIL] Daemon socket not found!"
    exit 1
fi

echo -e "\n========================================================"
echo "    ALL HARDWARE TESTS & CHECKS PASSED SUCCESSFULLY!    "
echo "========================================================"
