#!/usr/bin/env bash
# ==============================================================================
# Nullherz Baremetal CPU Core Isolation & Realtime Tuning Script
# ==============================================================================
# Automates ultra-low-latency baremetal tuning for Nullherz audio execution:
#   1. CPU Governor & C-State Power Pinning (latency lock)
#   2. Linux Kernel Boot Parameters (isolcpus, nohz_full, rcu_nocbs)
#   3. HugePages Allocation (MAP_HUGETLB / zero page-fault memory)
#   4. Realtime Scheduling Limits (/etc/security/limits.d/99-nullherz-realtime.conf)
# ==============================================================================

set -euo pipefail

COLOR_GREEN="\033[0;32m"
COLOR_YELLOW="\033[1;33m"
COLOR_RED="\033[0;31m"
COLOR_BLUE="\033[0;34m"
COLOR_RESET="\033[0m"

log_info() { echo -e "${COLOR_BLUE}[INFO]${COLOR_RESET} $1"; }
log_ok()   { echo -e "${COLOR_GREEN}[OK]${COLOR_RESET} $1"; }
log_warn() { echo -e "${COLOR_YELLOW}[WARN]${COLOR_RESET} $1"; }
log_err()  { echo -e "${COLOR_RED}[ERROR]${COLOR_RESET} $1"; }

show_status() {
    echo -e "${COLOR_BLUE}=== Nullherz Baremetal System Status ===${COLOR_RESET}"

    # 1. Kernel boot parameters
    echo -e "\n${COLOR_YELLOW}1. Kernel Boot Parameters (/proc/cmdline):${COLOR_RESET}"
    if [ -f /proc/cmdline ]; then
        cmdline=$(cat /proc/cmdline)
        echo "  CMDLINE: $cmdline"
        if echo "$cmdline" | grep -q "isolcpus"; then
            log_ok "isolcpus detected in boot parameters."
        else
            log_warn "isolcpus NOT found in boot parameters."
        fi
        if echo "$cmdline" | grep -q "nohz_full"; then
            log_ok "nohz_full detected in boot parameters."
        else
            log_warn "nohz_full NOT found in boot parameters."
        fi
    else
        log_warn "/proc/cmdline unavailable."
    fi

    # 2. CPU Governor
    echo -e "\n${COLOR_YELLOW}2. CPU Scaling Governors:${COLOR_RESET}"
    if [ -d /sys/devices/system/cpu/cpu0/cpufreq ]; then
        for gov in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
            cpu=$(echo "$gov" | awk -F'/' '{print $6}')
            val=$(cat "$gov")
            echo "  $cpu governor: $val"
        done
    else
        log_warn "cpufreq sysfs nodes not found."
    fi

    # 3. HugePages
    echo -e "\n${COLOR_YELLOW}3. HugePages Status (/proc/meminfo):${COLOR_RESET}"
    if [ -f /proc/meminfo ]; then
        grep -i "HugePages" /proc/meminfo | sed 's/^/  /' || true
    fi

    # 4. Realtime Limits
    echo -e "\n${COLOR_YELLOW}4. Current Realtime & Lock Memory Limits:${COLOR_RESET}"
    echo "  Max Realtime Priority (ulimit -r): $(ulimit -r)"
    echo "  Max Locked Memory (ulimit -l): $(ulimit -l) KB"
    echo ""
}

print_cmdline() {
    echo -e "${COLOR_BLUE}=== Recommended GRUB / Kernel Boot Parameters ===${COLOR_RESET}"
    echo "To isolate CPU cores 2 and 3 for dedicated low-latency audio execution, add to /etc/default/grub:"
    echo ""
    echo 'GRUB_CMDLINE_LINUX_DEFAULT="quiet splash isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3 intel_idle.max_cstate=0 processor.max_cstate=0 iommu=pt"'
    echo ""
    echo "Then update GRUB configuration:"
    echo "  sudo update-grub  # (or sudo grub-mkconfig -o /boot/grub/grub.cfg)"
}

apply_tuning() {
    if [ "$(id -u)" -ne 0 ]; then
        log_err "Tuning requires root privileges. Please run with sudo: sudo $0 --apply"
        exit 1
    fi

    log_info "Applying baremetal system tuning..."

    # 1. Set CPU scaling governors to 'performance'
    log_info "Setting CPU scaling governors to 'performance'..."
    for gov in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        if [ -f "$gov" ]; then
            echo "performance" > "$gov" || true
        fi
    done
    log_ok "CPU scaling governor set to performance."

    # 2. Disable CPU C-states if parameters are available
    log_info "Pinning C-state limits..."
    if [ -f /sys/module/intel_idle/parameters/max_cstate ]; then
        echo 0 > /sys/module/intel_idle/parameters/max_cstate || true
    fi
    if [ -f /sys/module/processor/parameters/max_cstate ]; then
        echo 0 > /sys/module/processor/parameters/max_cstate || true
    fi
    log_ok "C-state latency constraints applied."

    # 3. Configure HugePages (1024 x 2MB = 2GB)
    log_info "Configuring HugePages allocation (1024 pages)..."
    sysctl -w vm.nr_hugepages=1024 || true
    log_ok "HugePages configured."

    # 4. Configure /etc/security/limits.d/99-nullherz-realtime.conf
    log_info "Writing realtime execution limits to /etc/security/limits.d/99-nullherz-realtime.conf..."
    cat << 'EOF' > /etc/security/limits.d/99-nullherz-realtime.conf
# Nullherz Realtime Audio Execution Limits
@audio - rtprio 95
@audio - memlock unlimited
@audio - nice -20
@realtime - rtprio 95
@realtime - memlock unlimited
@realtime - nice -20
EOF
    log_ok "Realtime limits file /etc/security/limits.d/99-nullherz-realtime.conf updated."

    echo ""
    log_ok "Baremetal tuning applied successfully! Remember to add your user to the 'audio' group:"
    echo "  sudo usermod -aG audio \$USER"
    echo ""
}

usage() {
    echo "Usage: $0 [OPTION]"
    echo "Options:"
    echo "  --status        Display current system baremetal status"
    echo "  --apply         Apply CPU governor, HugePages, and RT limits tuning (requires sudo)"
    echo "  --print-cmdline Show recommended kernel boot parameters"
    echo "  --help          Display this help message"
}

case "${1:-}" in
    --status)
        show_status
        ;;
    --apply)
        apply_tuning
        ;;
    --print-cmdline)
        print_cmdline
        ;;
    --help|-h)
        usage
        ;;
    "")
        show_status
        echo ""
        print_cmdline
        ;;
    *)
        log_err "Unknown option: $1"
        usage
        exit 1
        ;;
esac
