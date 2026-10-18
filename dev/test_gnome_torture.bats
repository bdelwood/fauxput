#!/usr/bin/env bats
# Run only in a disposable GNOME Wayland session; these tests change displays.
# See dev/fedora-gnome/README.md for prerequisites and invocation.

set -o pipefail

setup_file() {
    export TORTURE_READY=0
    [ "${FAUXPUT_TEST_DISPOSABLE:-}" = 1 ] || {
        echo 'Set FAUXPUT_TEST_DISPOSABLE=1 inside a disposable GNOME VM.' >&2
        return 1
    }
    [ "${BATS_NUMBER_OF_PARALLEL_JOBS:-1}" -eq 1 ] || return 1
    local command
    for command in fauxput busctl jq pgrep find sudo timeout flock; do
        command -v "$command" >/dev/null || return 1
    done
    [ -n "${WAYLAND_DISPLAY:-}" ] && [ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ] || return 1
    [ -d "${XDG_RUNTIME_DIR:-}" ] || return 1
    exec 200>"$XDG_RUNTIME_DIR/fauxput-torture.lock"
    flock -n 200 || return 1
    [ -d /sys/kernel/config/vkms ] || return 1
    sudo -n true || return 1
    export GNOME_SHELL_PID_BEFORE
    GNOME_SHELL_PID_BEFORE=$(pgrep -u "$(id -u)" -x gnome-shell)
    [[ "$GNOME_SHELL_PID_BEFORE" =~ ^[0-9]+$ ]] || return 1
    mutter_state >/dev/null || return 1

    # Do not remove displays belonging to an existing session or test run.
    [ "$(vkms_instances)" -eq 0 ] || return 1
    fauxput status --json | jq -e '.instances | length == 0' >/dev/null || return 1
    export TORTURE_SETTLE_SEC=${TORTURE_SETTLE_SEC:-2}
    [[ "$TORTURE_SETTLE_SEC" =~ ^[0-9]+([.][0-9]+)?$ ]] || return 1
    export TORTURE_READY=1
    export TORTURE_PROBE="/sys/kernel/config/vkms/torture-probe-$$"
    sudo -n mkdir "$TORTURE_PROBE" || return 1
    sudo -n mkdir "$TORTURE_PROBE/connectors/0" || return 1
    [ -f "$TORTURE_PROBE/connectors/0/edid" ] || return 1
    [ -f "$TORTURE_PROBE/connectors/0/edid_enabled" ] || return 1
    sudo -n rmdir "$TORTURE_PROBE/connectors/0" "$TORTURE_PROBE" || return 1
    export HAS_HDR=0
    if fauxput up --help | grep -q -- '--hdr'; then
        export HAS_HDR=1
    fi
}

teardown_file() {
    [ "${TORTURE_READY:-0}" = 1 ] || return 0
    if [ -d "${TORTURE_PROBE:-}/connectors/0" ]; then
        sudo -n rmdir "$TORTURE_PROBE/connectors/0" || return 1
    fi
    if [ -d "${TORTURE_PROBE:-}" ]; then
        sudo -n rmdir "$TORTURE_PROBE" || return 1
    fi
}

setup() {
    [ "${TORTURE_READY:-0}" = 1 ] || return 1
    [ "$(vkms_instances)" -eq 0 ] || return 1
    export TORTURE_LAYOUT_BEFORE
    TORTURE_LAYOUT_BEFORE=$(mutter_layout) || return 1
}

teardown() {
    [ "${TORTURE_READY:-0}" = 1 ] || return 0
    local cleanup_failed=0
    if ! timeout 30s fauxput down; then
        cleanup_failed=1
        timeout 30s fauxput reset --yes || return 1
    fi
    sleep "$TORTURE_SETTLE_SEC"
    [ "$(pgrep -u "$(id -u)" -x gnome-shell)" = "$GNOME_SHELL_PID_BEFORE" ] || return 1
    [ "$(vkms_instances)" -eq 0 ] || return 1
    fauxput status --json | jq -e '.instances | length == 0' >/dev/null || return 1
    [ "$(mutter_layout)" = "$TORTURE_LAYOUT_BEFORE" ] || return 1
    [ "$cleanup_failed" -eq 0 ]
}

vkms_instances() {
    find /sys/kernel/config/vkms -mindepth 1 -maxdepth 1 -type d -name 'fauxput-*' -printf '.\n' | wc -l
}

mutter_state() {
    timeout 10s busctl --user --json=short call org.gnome.Mutter.DisplayConfig \
        /org/gnome/Mutter/DisplayConfig org.gnome.Mutter.DisplayConfig GetCurrentState
}

mutter_layout() {
    # Position, scale, transform, primary status, and monitor specifications.
    mutter_state | jq -ce '[.data[2][] | .[:6]] | sort'
}

mutter_monitor_count() {
    mutter_state | jq -er '.data[1] | length'
}

expect_clean_run() {
    local unexpected
    unexpected=$(printf '%s\n' "$output" \
        | grep -iE 'error:|warning:|\[WARN|\[ERROR' \
        | grep -vE '\[WARN[[:space:]]*\] kde connect failed' || true)
    if [ -n "$unexpected" ]; then
        printf '%s\n' "$unexpected" >&2
        return 1
    fi
}

up() {
    run timeout 30s fauxput up "$@"
    [ "$status" -eq 0 ] || { printf '%s\n' "$output" >&2; return 1; }
    expect_clean_run
}

@test "up creates one tracked configfs instance" {
    up --width 1920 --height 1080 --fps 60
    [ "$(vkms_instances)" -eq 1 ]
    fauxput status --json | jq -e '.instances | length == 1' >/dev/null
}

@test "down removes all tracked displays" {
    up --width 1920 --height 1080 --fps 60
    up --width 2560 --height 1440 --fps 60
    [ "$(vkms_instances)" -eq 2 ]
    run timeout 30s fauxput down
    [ "$status" -eq 0 ]
    expect_clean_run
    [ "$(vkms_instances)" -eq 0 ]
}

@test "down with nothing active succeeds" {
    run timeout 30s fauxput down
    [ "$status" -eq 0 ]
    expect_clean_run
}

@test "reset with nothing active succeeds" {
    run timeout 30s fauxput reset --yes
    [ "$status" -eq 0 ]
    expect_clean_run
}

@test "repeated up creates distinct displays" {
    up --width 1920 --height 1080 --fps 60
    up --width 1920 --height 1080 --fps 60
    [ "$(vkms_instances)" -eq 2 ]
    fauxput status --json | jq -e '.instances | length == 2' >/dev/null
}

@test "invalid dimensions fail without creating an instance" {
    run timeout 30s fauxput up --width 999999 --height 999999 --fps 60
    [ "$status" -ne 0 ] && [ "$status" -ne 124 ]
    [[ "$output" == *"dimensions out of supported range"* ]]
    [ "$(vkms_instances)" -eq 0 ]
}

@test "invalid width, height, and refresh are rejected" {
    local arguments width height fps
    for arguments in '0 1080 60' '-1 1080 60' '100000 1080 60' \
        '1920 0 60' '1920 100000 60' '1920 1080 0' '1920 1080 100000'; do
        read -r width height fps <<< "$arguments"
        run timeout 30s fauxput up --width "$width" --height "$height" --fps "$fps"
        [ "$status" -ne 0 ] && [ "$status" -ne 124 ] || return 1
        [ "$(vkms_instances)" -eq 0 ] || return 1
    done
}

@test "reset removes an orphan configfs instance" {
    sudo -n mkdir /sys/kernel/config/vkms/fauxput-orphan
    run timeout 30s fauxput reset --yes
    [ "$status" -eq 0 ]
    expect_clean_run
    [ "$(vkms_instances)" -eq 0 ]
}

@test "common modes configure without warnings" {
    local mode width height fps
    for mode in '1280 720 60' '1920 1080 144' '1920 1200 60' \
        '3440 1440 100' '3840 2160 60' '3840 2400 60' '1024 768 30'; do
        read -r width height fps <<< "$mode"
        up --width "$width" --height "$height" --fps "$fps" || return 1
        timeout 30s fauxput down || return 1
        sleep "$TORTURE_SETTLE_SEC"
    done
}

@test "modes above the EDID detailed-timing clock limit are rejected" {
    local mode width height fps
    for mode in '2560 1440 165' '3840 2160 120' '7680 4320 60'; do
        read -r width height fps <<< "$mode"
        run timeout 30s fauxput up --width "$width" --height "$height" --fps "$fps"
        [ "$status" -ne 0 ] && [ "$status" -ne 124 ] || return 1
        [[ "$output" == *"exceeds EDID 1.4 DTD limit"* ]] || return 1
        [ "$(vkms_instances)" -eq 0 ] || return 1
    done
}

@test "HDR displays configure without warnings" {
    [ "$HAS_HDR" -eq 1 ] || skip 'binary does not support --hdr'
    local mode width height fps
    for mode in '1920 1080 60' '3840 2160 60'; do
        read -r width height fps <<< "$mode"
        up --width "$width" --height "$height" --fps "$fps" --hdr || return 1
        timeout 30s fauxput down || return 1
        sleep "$TORTURE_SETTLE_SEC"
    done
}

@test "HDR does not bypass the detailed-timing clock limit" {
    [ "$HAS_HDR" -eq 1 ] || skip 'binary does not support --hdr'
    run timeout 30s fauxput up --width 3840 --height 2160 --fps 120 --hdr
    [ "$status" -ne 0 ] && [ "$status" -ne 124 ]
    [[ "$output" == *"exceeds EDID 1.4 DTD limit"* ]]
}

@test "Mutter sees the new connector and releases it after down" {
    local before
    before=$(mutter_monitor_count)
    up --width 1920 --height 1080 --fps 60
    sleep "$TORTURE_SETTLE_SEC"
    [ "$(mutter_monitor_count)" -eq "$((before + 1))" ]
    run timeout 30s fauxput down
    [ "$status" -eq 0 ]
    expect_clean_run
    sleep "$TORTURE_SETTLE_SEC"
    [ "$(mutter_monitor_count)" -eq "$before" ]
}

@test "down restores the layout after disabling real outputs" {
    up --width 1920 --height 1080 --fps 60 --primary --disable-real-outputs
    run timeout 30s fauxput down
    [ "$status" -eq 0 ]
    expect_clean_run
    sleep "$TORTURE_SETTLE_SEC"
    [ "$(mutter_layout)" = "$TORTURE_LAYOUT_BEFORE" ]
}

@test "ten up/down cycles leave no instances" {
    local i
    for ((i = 0; i < 10; i++)); do
        up --width 1920 --height 1080 --fps 60 || return 1
        timeout 30s fauxput down || return 1
        sleep "$TORTURE_SETTLE_SEC"
        [ "$(vkms_instances)" -eq 0 ] || return 1
        [ "$(pgrep -u "$(id -u)" -x gnome-shell)" = "$GNOME_SHELL_PID_BEFORE" ] || return 1
    done
}

@test "four simultaneous displays configure without warnings" {
    local i
    for ((i = 0; i < 4; i++)); do
        up --width 1280 --height 720 --fps 60 || return 1
        sleep "$TORTURE_SETTLE_SEC"
    done
    [ "$(vkms_instances)" -eq 4 ]
}

@test "reset removes instances when the state file is corrupt" {
    up --width 1920 --height 1080 --fps 60
    printf '%s\n' '{ invalid json' | sudo -n tee /run/fauxput/active.json >/dev/null
    run timeout 30s fauxput reset --yes
    [ "$status" -eq 0 ]
    [ "$(vkms_instances)" -eq 0 ]
}
