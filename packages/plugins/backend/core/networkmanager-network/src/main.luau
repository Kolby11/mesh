-- NetworkManager backend.
-- Dominant provider for mesh.network. Implements the base interface contract
-- (available, wifi_enabled, connections, devices) and emits additive runtime
-- extras (networks from the last scan) on every poll.
-- Handles connect/disconnect/set_wifi_enabled/wifi_scan commands.

state = { available = false, wifi_enabled = false, connections = {}, devices = {}, networks = {} }

-- Cache the last successful wifi scan so the networks list is available
-- even between explicit scan commands.
local last_networks = {}

function init()
    mesh.service.set_poll_interval(15000)
end

local function split_line(line, sep)
    sep = sep or ":"
    local res = {}
    if line == nil then return res end
    for part in (line .. sep):gmatch("(.-)" .. sep) do
        table.insert(res, part)
    end
    return res
end

local function fetch_connections()
    local result = mesh.exec("nmcli", { "-t", "-f", "UUID,NAME,TYPE,DEVICE,STATE,SSID,SIGNAL", "connection", "show" })
    if not result.success then return nil end
    local conns = {}
    for line in result.stdout:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local id     = cols[1] or ''
        local name   = cols[2] or ''
        local kind   = cols[3] or ''
        local device = cols[4] or ''
        local state  = cols[5] or ''
        local signal = tonumber(cols[7]) or 0
        local strength = signal > 0 and math.floor(signal + 0.5) or 0
        local active = (state == 'activated' or state == 'connected' or device ~= '')
        table.insert(conns, { id = id, name = name, kind = kind, strength = strength, active = active })
    end
    return conns
end

local function fetch_net_devices()
    local result = mesh.exec("nmcli", { "-t", "-f", "DEVICE,TYPE,STATE", "device" })
    if not result.success then return nil end
    local devices = {}
    for line in result.stdout:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local id    = cols[1] or ''
        local kind  = cols[2] or ''
        local state = cols[3] or ''
        table.insert(devices, { id = id, name = id, kind = kind, state = state })
    end
    return devices
end

local function fetch_bluetooth_devices()
    local devices = {}
    local seen = {}
    local connected = mesh.exec("bluetoothctl", { "devices", "Connected" })
    if connected.success then
        for line in connected.stdout:gmatch('([^\n]+)') do
            local mac, name = line:match('Device%s+([%w:]+)%s+(.*)')
            if mac and name then
                table.insert(devices, { id = mac, name = name, kind = 'bluetooth', state = 'connected' })
                seen[mac] = true
            end
        end
    end
    local paired = mesh.exec("bluetoothctl", { "devices", "Paired" })
    if paired.success then
        for line in paired.stdout:gmatch('([^\n]+)') do
            local mac, name = line:match('Device%s+([%w:]+)%s+(.*)')
            if mac and name and not seen[mac] then
                table.insert(devices, { id = mac, name = name, kind = 'bluetooth', state = 'paired' })
                seen[mac] = true
            end
        end
    end
    return devices
end

-- Determine whether the Wi-Fi radio is currently enabled via nmcli.
local function fetch_wifi_enabled()
    local result = mesh.exec("nmcli", { "radio", "wifi" })
    if not result.success then return false end
    local out = result.stdout:match('^%s*(.-)%s*$') or ''
    return out == 'enabled'
end

local function fetch_wifi_networks(rescan)
    if rescan then
        mesh.exec("nmcli", { "device", "wifi", "rescan" })
    end
    local result = mesh.exec("nmcli", { "-t", "-f", "SSID,SIGNAL,BSSID", "device", "wifi", "list" })
    if not result.success then
        return last_networks, result
    end
    local networks = {}
    for line in result.stdout:gmatch('([^\n]+)') do
        local ssid, signal, bssid = line:match('^(.-):(%d+):(.+)$')
        if not ssid then
            local cols = split_line(line, '|')
            ssid = cols[1] or ''
            signal = cols[2] or '0'
            bssid = cols[3] or ''
        end
        bssid = (bssid or ''):gsub('\\:', ':')
        local strength = math.floor((tonumber(signal) or 0) + 0.5)
        local id = bssid ~= '' and ('wifi:' .. bssid) or ('wifi:' .. ssid)
        table.insert(networks, { id = id, name = ssid or '', kind = 'wifi', strength = strength, active = false })
    end
    last_networks = networks
    return networks, result
end

local function has_control()
    return mesh.service.has_capability('service.network.control')
end

local function is_bluetooth_mac(id)
    return type(id) == 'string'
        and id:match('^%x%x:%x%x:%x%x:%x%x:%x%x:%x%x$') ~= nil
end

local function is_uuid(id)
    if type(id) ~= 'string' or #id ~= 36 then
        return false
    end
    if id:sub(9, 9) ~= '-' or id:sub(14, 14) ~= '-' or id:sub(19, 19) ~= '-' or id:sub(24, 24) ~= '-' then
        return false
    end
    local compact = id:gsub('-', '')
    return #compact == 32 and compact:match('^%x+$') ~= nil
end

local warned = false
local function set_unavailable_state()
    state = {
        available    = false,
        wifi_enabled = false,
        connections  = {},
        devices      = {},
        networks     = last_networks,
    }
end

local function refresh_state()
    local conns = fetch_connections()
    if not conns then
        if not warned then
            mesh.log.warn('NetworkManager backend: nmcli query failed')
            warned = true
        end
        set_unavailable_state()
        return false
    end
    warned = false
    local net_devices = fetch_net_devices() or {}
    local bt_devices = fetch_bluetooth_devices()
    for _, d in ipairs(bt_devices) do
        table.insert(net_devices, d)
    end
    local wifi_on = fetch_wifi_enabled()
    local networks = fetch_wifi_networks(false)

    -- Emit base interface state fields plus runtime-defined extras.
    -- Networks are additive surface that richer UIs can use.
    state = {
        available    = true,
        wifi_enabled = wifi_on,
        connections  = conns,
        devices      = net_devices,
        networks     = networks,
    }
    return true
end

function on_poll()
    refresh_state()
end

function on_command_connect()
    if not has_control() then
        return { ok = false, error = 'permission denied' }
    end
    local payload = mesh.service.payload()
    local id = payload.connection_id or payload.id or ''
    if id == '' then
        return { ok = false, error = 'missing connection_id' }
    end
    local result = nil
    local kind = payload.kind or payload.connection_kind or ''
    if is_bluetooth_mac(id) and (kind == '' or kind == 'bluetooth') then
        result = mesh.exec('bluetoothctl', { 'connect', id })
    elseif is_uuid(id) then
        result = mesh.exec('nmcli', { 'connection', 'up', 'uuid', id })
    else
        return { ok = false, error = 'invalid connection_id' }
    end
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_disconnect()
    if not has_control() then
        return { ok = false, error = 'permission denied' }
    end
    local payload = mesh.service.payload()
    local id = payload.connection_id or payload.id or ''
    if id == '' then
        return { ok = false, error = 'missing connection_id' }
    end
    local result = nil
    local kind = payload.kind or payload.connection_kind or ''
    if is_bluetooth_mac(id) and (kind == '' or kind == 'bluetooth') then
        result = mesh.exec('bluetoothctl', { 'disconnect', id })
    elseif is_uuid(id) then
        result = mesh.exec('nmcli', { 'connection', 'down', 'uuid', id })
    else
        return { ok = false, error = 'invalid connection_id' }
    end
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_wifi_scan()
    local networks, result = fetch_wifi_networks(true)
    if not result.success then
        set_unavailable_state()
        return { ok = false, error = result.stderr }
    end
    state = {
        available    = true,
        wifi_enabled = fetch_wifi_enabled(),
        connections  = fetch_connections() or {},
        devices      = {},
        networks     = networks,
    }
    return { ok = true }
end

function on_command_set_wifi_enabled()
    if not has_control() then
        return { ok = false, error = 'permission denied' }
    end
    local payload = mesh.service.payload()
    local wifi_state = payload.enabled and "on" or "off"
    local result = mesh.exec("nmcli", { "radio", "wifi", wifi_state })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end
