-- UPower battery backend.
-- Polls UPower via its CLI and emits battery state as JSON.

mesh.service.set_poll_interval(15000)

function on_poll()
    mesh.exec_shell([=[upower -i /org/freedesktop/UPower/devices/DisplayDevice | awk '
BEGIN {
    present = "false"
    charging = "false"
    percent = 0
    time_remaining = 0
    time_to_full = 0
    power_draw = 0
    energy_now = 0
    energy_full = 0
}

/^[[:space:]]*present:/ {
    if ($2 == "yes") present = "true"
}

/^[[:space:]]*state:/ {
    if ($2 == "charging" || $2 == "pending-charge" || $2 == "fully-charged") charging = "true"
}

/^[[:space:]]*percentage:/ {
    gsub(/%/, "", $2)
    percent = $2 + 0
}

/^[[:space:]]*time to empty:/ {
    time_remaining = parse_duration($4, $5)
}

/^[[:space:]]*time to full:/ {
    time_to_full = parse_duration($4, $5)
}

/^[[:space:]]*energy-rate:/ {
    power_draw = parse_decimal($2)
}

/^[[:space:]]*energy:/ {
    energy_now = parse_decimal($2)
}

/^[[:space:]]*energy-full:/ {
    energy_full = parse_decimal($2)
}

function parse_duration(value, unit) {
    gsub(/,/, "", unit)
    if (unit ~ /^hour/) return int((value * 60) + 0.5)
    if (unit ~ /^minute/) return int(value + 0.5)
    if (unit ~ /^second/) return int((value / 60) + 0.5)
    if (unit ~ /^day/) return int((value * 1440) + 0.5)
    return 0
}

function parse_decimal(value) {
    gsub(/,/, ".", value)
    return value + 0
}

END {
    printf("{\"available\":%s,\"level\":%.2f,\"charging\":%s,\"time_remaining_minutes\":%d,\"time_to_full_minutes\":%d,\"power_draw_watts\":%.2f,\"energy_now_watt_hours\":%.2f,\"energy_full_watt_hours\":%.2f,\"source_plugin\":\"@mesh/upower\"}\n", present, percent / 100.0, charging, time_remaining, time_to_full, power_draw, energy_now, energy_full)
}
']=])
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end
