-- UPower-backed power/battery backend.
-- Discovers the first battery device via `upower -e`, then polls
-- `upower -i <path>` and parses the textual report into the
-- mesh.power state contract.

state = {
    available = false,
    level = 0,
    charging = false,
    time_remaining_minutes = 0,
    time_to_full_minutes = 0,
    power_draw_watts = 0,
    energy_now_watt_hours = 0,
    energy_full_watt_hours = 0,
}

local battery_path = nil
local last_availability = nil
local last_snapshot = nil

local function debug_info(message)
    mesh.log.info("[upower-power] " .. message)
end

local function debug_warn(message)
    mesh.log.warn("[upower-power] " .. message)
end

local function discover_battery_path()
    local result = mesh.exec("upower", { "-e" })
    if not result.success then
        return nil
    end
    for line in (result.stdout or ""):gmatch("([^\n]+)") do
        local trimmed = line:match("^%s*(.-)%s*$")
        if trimmed and trimmed:find("/battery_", 1, true) then
            return trimmed
        end
    end
    return nil
end

-- Convert "39.1 minutes" / "2.3 hours" / "30 seconds" to minutes.
local function parse_duration_to_minutes(raw)
    if type(raw) ~= "string" then
        return 0
    end
    local value, unit = raw:match("([%d%.]+)%s*(%a+)")
    local number = tonumber(value)
    if not number or not unit then
        return 0
    end
    unit = unit:lower()
    if unit:find("hour", 1, true) then
        return number * 60
    elseif unit:find("minute", 1, true) then
        return number
    elseif unit:find("second", 1, true) then
        return number / 60
    elseif unit:find("day", 1, true) then
        return number * 60 * 24
    end
    return 0
end

local function parse_battery_report(report)
    local parsed = {
        available = true,
        level = 0,
        charging = false,
        time_remaining_minutes = 0,
        time_to_full_minutes = 0,
        power_draw_watts = 0,
        energy_now_watt_hours = 0,
        energy_full_watt_hours = 0,
    }

    -- Percentage is the source of truth for level. Capacity is health, not level.
    local percentage = report:match("percentage:%s*([%d%.]+)")
    if percentage then
        parsed.level = tonumber(percentage) or 0
    end

    local state_value = report:match("state:%s*(%S+)")
    if state_value then
        parsed.charging = state_value == "charging" or state_value == "fully-charged"
    end

    local energy_now = report:match("energy:%s*([%d%.]+)%s*Wh")
    if energy_now then
        parsed.energy_now_watt_hours = tonumber(energy_now) or 0
    end

    local energy_full = report:match("energy%-full:%s*([%d%.]+)%s*Wh")
    if energy_full then
        parsed.energy_full_watt_hours = tonumber(energy_full) or 0
    end

    local energy_rate = report:match("energy%-rate:%s*([%d%.]+)%s*W")
    if energy_rate then
        parsed.power_draw_watts = tonumber(energy_rate) or 0
    end

    local time_to_empty = report:match("time to empty:%s*([^\n]+)")
    if time_to_empty then
        parsed.time_remaining_minutes = parse_duration_to_minutes(time_to_empty)
    end

    local time_to_full = report:match("time to full:%s*([^\n]+)")
    if time_to_full then
        parsed.time_to_full_minutes = parse_duration_to_minutes(time_to_full)
    end

    return parsed
end

local function read_state()
    if battery_path == nil then
        battery_path = discover_battery_path()
        if battery_path == nil then
            return nil
        end
        debug_info("discovered battery at " .. battery_path)
    end

    local result = mesh.exec("upower", { "-i", battery_path })
    if not result.success or (result.stdout or "") == "" then
        -- Device may have been removed; rediscover next tick.
        battery_path = nil
        return nil
    end

    return parse_battery_report(result.stdout)
end

local function unavailable_state()
    return {
        available = false,
        level = 0,
        charging = false,
        time_remaining_minutes = 0,
        time_to_full_minutes = 0,
        power_draw_watts = 0,
        energy_now_watt_hours = 0,
        energy_full_watt_hours = 0,
    }
end

local function refresh_state(self)
    local next_state = read_state()
    if next_state then
        state = next_state
    else
        state = unavailable_state()
    end

    if last_availability ~= state.available then
        last_availability = state.available
        if state.available then
            debug_info("power became available")
        else
            debug_warn("power became unavailable")
        end
    end

    local snapshot = string.format(
        "available=%s level=%.1f%% charging=%s rate=%.2fW",
        tostring(state.available),
        state.level,
        tostring(state.charging),
        state.power_draw_watts
    )
    if snapshot ~= last_snapshot then
        last_snapshot = snapshot
        debug_info("state " .. snapshot)
        if state.available and self ~= nil and self.BatteryChanged ~= nil then
            self.BatteryChanged:fire({
                level = state.level,
                charging = state.charging,
            })
        end
    end
end

function start(self)
    -- Battery state changes slowly. Poll every 10s to keep CPU and exec
    -- churn low; upower's time-to-full / time-remaining estimates only
    -- update every ~30s anyway.
    mesh.service.set_poll_interval(10000)
    debug_info("initialized with 10s poll interval")
    refresh_state(self)
end

function on_poll(self)
    refresh_state(self)
end
