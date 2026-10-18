-- NetworkManager provider.
--
-- Native D-Bus signals mark state dirty; a short poll coalesces bursts.
-- nmcli remains the domain adapter and the monitor/poll fallback.

local IDLE_POLL_MS = 60000
local DIRTY_POLL_MS = 400

state = {
    available = false, connectivity = "none", kind = "none", wifi_enabled = false,
    ssid = "", signal = 0, interface_name = "", networks = {},
}

local function nmcli(args)
    return mesh.exec("nmcli", args)
end

local function lines(text)
    local out = {}
    for line in (text or ""):gmatch("[^\n]+") do
        out[#out + 1] = line
    end
    return out
end

-- Terse output escapes ':' and '\' inside values with a backslash.
local function terse_fields(line)
    local fields, current, index = {}, {}, 1
    while index <= #line do
        local char = line:sub(index, index)
        if char == "\\" and index < #line then
            current[#current + 1] = line:sub(index + 1, index + 1)
            index += 2
        elseif char == ":" then
            fields[#fields + 1] = table.concat(current)
            current = {}
            index += 1
        else
            current[#current + 1] = char
            index += 1
        end
    end
    fields[#fields + 1] = table.concat(current)
    return fields
end

local function read_state()
    local connectivity = nmcli({ "networking", "connectivity" })
    if not connectivity.success then
        return nil
    end
    local radio = nmcli({ "radio", "wifi" })
    local devices = nmcli({ "-t", "-f", "TYPE,STATE,CONNECTION,DEVICE", "device" })

    local next_state = {
        available = true,
        connectivity = (connectivity.stdout or ""):match("%a+") or "none",
        kind = "none",
        wifi_enabled = (radio.stdout or ""):match("enabled") ~= nil,
        ssid = "", signal = 0, interface_name = "", networks = {},
    }

    for _, line in lines(devices.stdout) do
        local fields = terse_fields(line)
        local kind, device_state, connection, device = fields[1], fields[2], fields[3], fields[4]
        if device_state == "connected" and (kind == "wifi" or kind == "ethernet") and next_state.kind ~= "ethernet" then
            next_state.kind = kind
            next_state.interface_name = device or ""
            if kind == "wifi" then
                next_state.ssid = connection or ""
            end
        end
    end

    if next_state.wifi_enabled then
        local list = nmcli({ "-t", "-f", "ACTIVE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "no" })
        local seen = {}
        for _, line in lines(list.stdout) do
            local fields = terse_fields(line)
            local ssid = fields[2] or ""
            local connected = fields[1] == "yes"
            local signal = math.clamp(tonumber(fields[3]) or 0, 0, 100)
            if connected and next_state.kind == "wifi" then
                next_state.signal = signal
            end
            if ssid ~= "" and not seen[ssid] then
                seen[ssid] = true
                next_state.networks[#next_state.networks + 1] = {
                    id = ssid, ssid = ssid, signal = signal,
                    secured = (fields[4] or "") ~= "" and fields[4] ~= "--",
                    connected = connected,
                }
            end
        end
    end
    return next_state
end

local dirty = false
local monitor_program = nil
local native_monitor = false

local function refresh()
    state = read_state() or {
        available = false, connectivity = "none", kind = "none", wifi_enabled = false,
        ssid = "", signal = 0, interface_name = "", networks = {},
    }
end

function start(self)
    mesh.service.set_poll_interval(IDLE_POLL_MS)
    local monitor = mesh.dbus_subscribe("system", {
        sender = "org.freedesktop.NetworkManager",
        path_namespace = "/org/freedesktop/NetworkManager",
    })
    if monitor then
        monitor_program = monitor.program
        native_monitor = true
    else
        monitor = mesh.exec_stream("nmcli", { "monitor" })
        monitor_program = monitor and monitor.program or nil
        if not monitor then mesh.service.set_poll_interval(5000) end
    end
    refresh()
end

local state_properties = {
    State = true, Connectivity = true, WirelessEnabled = true,
    PrimaryConnection = true, ActiveConnections = true, Devices = true,
    Interface = true, DeviceType = true, ActiveConnection = true, Managed = true,
    ActiveAccessPoint = true, AccessPoints = true, Strength = true,
    Ssid = true, Flags = true, WpaFlags = true, RsnFlags = true,
}
local state_signals = {
    StateChanged = true, DeviceAdded = true, DeviceRemoved = true,
    AccessPointAdded = true, AccessPointRemoved = true,
}

local function changes_state(line)
    local signal = mesh.json.decode(line)
    if signal.member ~= "PropertiesChanged" then
        return state_signals[signal.member] == true
    end
    local fields = signal.body or {}
    local changed = fields[2] and fields[2].value or {}
    for key in pairs(changed) do
        if state_properties[key] then return true end
    end
    local invalidated = fields[3] and fields[3].value or {}
    for _, key in ipairs(invalidated) do
        if state_properties[key] then return true end
    end
    return false
end

function on_stream_line(self, program, line)
    if program ~= monitor_program or dirty then
        return
    end
    if native_monitor and not changes_state(line) then return end
    dirty = true
    mesh.service.set_poll_interval(DIRTY_POLL_MS)
end

function on_stream_closed(self, program, _success)
    if program ~= monitor_program then return end
    monitor_program = nil
    if native_monitor then
        native_monitor = false
        local monitor = mesh.exec_stream("nmcli", { "monitor" })
        monitor_program = monitor and monitor.program or nil
    end
    mesh.service.set_poll_interval(monitor_program and IDLE_POLL_MS or 5000)
end

function on_poll(self)
    refresh()
    if dirty then
        dirty = false
        mesh.service.set_poll_interval(IDLE_POLL_MS)
    end
end

local function control(args)
    local result = nmcli(args)
    refresh()
    return { ok = result.success, error = result.stderr }
end

function on_command_set_wifi_enabled(self)
    local enabled = mesh.service.payload().enabled == true
    return control({ "radio", "wifi", enabled and "on" or "off" })
end

function on_command_scan(self)
    return control({ "device", "wifi", "rescan" })
end

function on_command_connect(self)
    local id = tostring(mesh.service.payload().id or "")
    if id == "" then
        return { ok = false, error = "missing network id" }
    end
    return control({ "device", "wifi", "connect", id })
end
