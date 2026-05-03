-- UPower battery backend.
-- Polls UPower via its CLI and emits battery state as JSON.

state = {
    available = false,
    level = 0,
    charging = false,
    time_remaining_minutes = 0,
    time_to_full_minutes = 0,
}

function init()
    mesh.service.set_poll_interval(15000)
end

local function parse_decimal(value)
    if type(value) ~= "string" then
        return 0
    end
    local raw = value:match("([%d%.,]+)")
    if raw == nil then
        return 0
    end
    return tonumber(raw:gsub(",", ".")) or 0
end

local function parse_duration_minutes(value)
    if type(value) ~= "string" then
        return 0
    end
    local amount, unit = value:match("([%d%.,]+)%s+(%a+)")
    amount = tonumber((amount or "0"):gsub(",", ".")) or 0
    unit = unit or ""
    if unit:match("^hour") then
        return math.floor((amount * 60) + 0.5)
    end
    if unit:match("^minute") then
        return math.floor(amount + 0.5)
    end
    if unit:match("^second") then
        return math.floor((amount / 60) + 0.5)
    end
    if unit:match("^day") then
        return math.floor((amount * 1440) + 0.5)
    end
    return 0
end

local function parse_display_device(stdout)
    local state = {
        available = false,
        level = 0,
        charging = false,
        time_remaining_minutes = 0,
        time_to_full_minutes = 0,
        power_draw_watts = 0,
        energy_now_watt_hours = 0,
        energy_full_watt_hours = 0,
    }

    for line in (stdout or ""):gmatch("([^\n]+)") do
        local key, value = line:match("^%s*([^:]+):%s*(.-)%s*$")
        if key == "present" then
            state.available = value == "yes"
        elseif key == "state" then
            state.charging = value == "charging" or value == "pending-charge" or value == "fully-charged"
        elseif key == "percentage" then
            state.level = parse_decimal(value)
        elseif key == "time to empty" then
            state.time_remaining_minutes = parse_duration_minutes(value)
        elseif key == "time to full" then
            state.time_to_full_minutes = parse_duration_minutes(value)
        elseif key == "energy-rate" then
            state.power_draw_watts = parse_decimal(value)
        elseif key == "energy" then
            state.energy_now_watt_hours = parse_decimal(value)
        elseif key == "energy-full" then
            state.energy_full_watt_hours = parse_decimal(value)
        end
    end

    return state
end

function on_poll()
    local result = mesh.exec("upower", { "-i", "/org/freedesktop/UPower/devices/DisplayDevice" })
    if result.success then
        state = parse_display_device(result.stdout)
    else
        state = {
            available = false,
            level = 0,
            charging = false,
            time_remaining_minutes = 0,
            time_to_full_minutes = 0,
        }
    end
end
