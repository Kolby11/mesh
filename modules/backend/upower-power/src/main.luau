-- UPower-backed power/battery backend.
-- Finds the first battery device by introspecting UPower's device tree and
-- reads its properties over D-Bus into the mesh.power state contract. UPower
-- signals refresh promptly on power-source and device changes; a slow poll
-- keeps battery level and estimates current.

state = {
    available = false,
    level = 0,
    charging = false,
    time_remaining_minutes = 0,
    time_to_full_minutes = 0,
    power_draw_watts = 0,
    energy_now_watt_hours = 0,
    energy_full_watt_hours = 0,
    health_percent = 0,
}

local UPOWER = "org.freedesktop.UPower"
local DEVICES = "/org/freedesktop/UPower/devices"
local DEVICE_INTERFACE = "org.freedesktop.UPower.Device"
-- UPower.Device.State: 1 charging, 4 fully charged.
local CHARGING_STATES = { [1] = true, [4] = true }

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
    local reply = mesh.dbus_call("system", {
        destination = UPOWER,
        path = DEVICES,
        interface = "org.freedesktop.DBus.Introspectable",
        member = "Introspect",
    })
    if not reply.success then
        return nil
    end
    local xml = reply.body[1] and reply.body[1].value or ""
    local name = xml:match('<node%s+name="(battery_[^"]+)"')
    return name and (DEVICES .. "/" .. name) or nil
end

local function device_properties(path)
    local reply = mesh.dbus_call("system", {
        destination = UPOWER,
        path = path,
        interface = "org.freedesktop.DBus.Properties",
        member = "GetAll",
        signature = "s",
        args = { DEVICE_INTERFACE },
    })
    if not reply.success then
        return nil
    end
    local properties = {}
    for name, entry in pairs(reply.body[1] and reply.body[1].value or {}) do
        properties[name] = entry.value
    end
    return properties
end

local function read_state()
    if battery_path == nil then
        battery_path = discover_battery_path()
        if battery_path == nil then
            return nil
        end
        debug_info("discovered battery at " .. battery_path)
    end

    local device = device_properties(battery_path)
    if device == nil or device.IsPresent == false then
        -- Device may have been removed; rediscover next tick.
        battery_path = nil
        return nil
    end

    return {
        available = true,
        level = tonumber(device.Percentage) or 0,
        charging = CHARGING_STATES[device.State] == true,
        time_remaining_minutes = (tonumber(device.TimeToEmpty) or 0) / 60,
        time_to_full_minutes = (tonumber(device.TimeToFull) or 0) / 60,
        power_draw_watts = tonumber(device.EnergyRate) or 0,
        energy_now_watt_hours = tonumber(device.Energy) or 0,
        energy_full_watt_hours = tonumber(device.EnergyFull) or 0,
        health_percent = tonumber(device.Capacity) or 0,
    }
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
        health_percent = 0,
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

local POLL_MS = 10000
local monitor = nil

function start(self)
    mesh.service.set_poll_interval(POLL_MS)
    monitor = mesh.dbus_subscribe("system", {
        sender = UPOWER,
        path_namespace = "/org/freedesktop/UPower",
    })
    refresh_state(self)
end

function on_stream_batch(self, program, lines)
    if monitor == nil or program ~= monitor.program then
        return
    end
    for _, line in ipairs(lines) do
        local signal = mesh.json.decode(line)
        if signal.member == "DeviceAdded" or signal.member == "DeviceRemoved"
            or (signal.path or ""):find("line_power", 1, true)
        then
            refresh_state(self)
            return
        end
    end
end

function on_stream_closed(self, program, _success)
    if monitor ~= nil and program == monitor.program then
        monitor = nil
    end
end

function on_poll(self)
    refresh_state(self)
end
