-- NetworkManager provider.
--
-- State is read from NetworkManager's D-Bus properties; its signals mark state
-- dirty and a short poll coalesces bursts. nmcli performs controls only.

local IDLE_POLL_MS = 60000
local DIRTY_POLL_MS = 400

local NM = "org.freedesktop.NetworkManager"
local NM_PATH = "/org/freedesktop/NetworkManager"
local DEVICE = NM .. ".Device"
local WIRELESS = NM .. ".Device.Wireless"
local ACCESS_POINT = NM .. ".AccessPoint"
-- NMDeviceType and NMDeviceState values.
local DEVICE_KINDS = { [1] = "ethernet", [2] = "wifi" }
local DEVICE_ACTIVATED = 100
-- NMConnectivityState: 1 none, 2 portal, 3 limited, 4 full.
local CONNECTIVITY = { [2] = "portal", [3] = "limited", [4] = "full" }

state = {
    available = false, connectivity = "none", kind = "none", wifi_enabled = false,
    ssid = "", signal = 0, interface_name = "", networks = {},
}

local function nmcli(args)
    return mesh.exec("nmcli", args)
end

local function properties(path, interface)
    local reply = mesh.dbus_call("system", {
        destination = NM,
        path = path,
        interface = "org.freedesktop.DBus.Properties",
        member = "GetAll",
        signature = "s",
        args = { interface },
    })
    if not reply.success then
        mesh.log.warn("[networkmanager-network] " .. interface .. " read failed: " .. tostring(reply.error))
        return nil
    end
    local values = {}
    for name, entry in pairs(reply.body[1] and reply.body[1].value or {}) do
        values[name] = entry.value
    end
    return values
end

local function ssid_text(bytes)
    if type(bytes) ~= "table" or #bytes == 0 then
        return ""
    end
    return string.char(table.unpack(bytes))
end

local function access_points(wireless, next_state, connected_path)
    local seen, networks = {}, {}
    for _, path in ipairs(wireless.AccessPoints or {}) do
        local point = properties(path, ACCESS_POINT)
        local ssid = point and ssid_text(point.Ssid) or ""
        if point then
            local signal = math.clamp(tonumber(point.Strength) or 0, 0, 100)
            local connected = path == connected_path
            if connected then
                next_state.signal = signal
                next_state.ssid = ssid
            end
            if ssid ~= "" and not seen[ssid] then
                seen[ssid] = true
                networks[#networks + 1] = {
                    id = ssid, ssid = ssid, signal = signal,
                    secured = (point.Flags or 0) % 2 == 1 or (point.WpaFlags or 0) ~= 0
                        or (point.RsnFlags or 0) ~= 0,
                    connected = connected,
                }
            end
        end
    end
    table.sort(networks, function(a, b)
        if a.connected ~= b.connected then return a.connected end
        return a.signal > b.signal
    end)
    next_state.networks = networks
end

local function read_state()
    local manager = properties(NM_PATH, NM)
    if manager == nil then
        return nil
    end
    local next_state = {
        available = true,
        connectivity = CONNECTIVITY[manager.Connectivity] or "none",
        kind = "none",
        wifi_enabled = manager.WirelessEnabled == true,
        ssid = "", signal = 0, interface_name = "", networks = {},
    }

    local wireless_device = nil
    local wifi_connected = false
    for _, path in ipairs(manager.Devices or {}) do
        local device = properties(path, DEVICE)
        local kind = device and DEVICE_KINDS[device.DeviceType]
        if kind == "wifi" and wireless_device == nil then
            wireless_device = path
        end
        if kind and device.State == DEVICE_ACTIVATED and next_state.kind ~= "ethernet" then
            next_state.kind = kind
            next_state.interface_name = device.Interface or ""
            if kind == "wifi" then
                wireless_device = path
                wifi_connected = true
            end
        end
    end

    if next_state.wifi_enabled and wireless_device then
        local wireless = properties(wireless_device, WIRELESS)
        if wireless then
            local active = wifi_connected and wireless.ActiveAccessPoint or nil
            access_points(wireless, next_state, active)
            if next_state.kind ~= "wifi" then
                next_state.signal = 0
                next_state.ssid = ""
            end
        end
    end
    return next_state
end

local dirty = false
local monitor = nil

local function refresh()
    state = read_state() or {
        available = false, connectivity = "none", kind = "none", wifi_enabled = false,
        ssid = "", signal = 0, interface_name = "", networks = {},
    }
end

function start(self)
    mesh.service.set_poll_interval(IDLE_POLL_MS)
    monitor = mesh.dbus_subscribe("system", {
        sender = NM,
        path_namespace = NM_PATH,
    })
    if not monitor then mesh.service.set_poll_interval(5000) end
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
    if monitor == nil or program ~= monitor.program or dirty then
        return
    end
    if not changes_state(line) then return end
    dirty = true
    mesh.service.set_poll_interval(DIRTY_POLL_MS)
end

function on_stream_closed(self, program, _success)
    if monitor == nil or program ~= monitor.program then return end
    monitor = nil
    mesh.service.set_poll_interval(5000)
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
