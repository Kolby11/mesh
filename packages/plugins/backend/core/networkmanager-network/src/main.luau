-- NetworkManager backend.
-- Dominant provider for mesh.network. Implements the base interface contract
-- (available, wifi_enabled, connections, devices) and emits additive runtime
-- extras (networks from the last scan, source_plugin identifier) on every poll.
-- Handles connect/disconnect/set_wifi_enabled/wifi_scan commands.

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

-- Cache the last successful wifi scan so the networks list is available
-- even between explicit scan commands.
local last_networks = {}

local warned = false
function on_poll()
    local conns = fetch_connections()
    if not conns then
        if not warned then
            mesh.log.warn('NetworkManager backend: nmcli query failed')
            warned = true
        end
        mesh.service.emit_unavailable()
        return
    end
    warned = false
    local net_devices = fetch_net_devices() or {}
    local bt_devices = fetch_bluetooth_devices()
    for _, d in ipairs(bt_devices) do
        table.insert(net_devices, d)
    end
    local wifi_on = fetch_wifi_enabled()

    -- Emit base interface state fields plus runtime-defined extras.
    -- Networks and source_plugin are additive surface that richer UIs can use.
    mesh.service.emit({
        available    = true,
        wifi_enabled = wifi_on,
        connections  = conns,
        devices      = net_devices,
        networks     = last_networks,
        source_plugin = '@mesh/networkmanager',
    })
end

function on_command_connect()
    if not has_control() then
        mesh.service.emit({ ok = false, error = 'permission denied' })
        return
    end
    local payload = mesh.service.payload()
    local id = payload.connection_id or payload.id or ''
    if id == '' then
        mesh.service.emit({ ok = false, error = 'missing connection_id' })
        return
    end
    if is_bluetooth_mac(id) then
        local result = mesh.exec('bluetoothctl', { 'connect', id })
        mesh.service.emit({ ok = result.success, error = result.stderr })
    elseif is_uuid(id) then
        local result = mesh.exec('nmcli', { 'connection', 'up', 'uuid', id })
        mesh.service.emit({ ok = result.success, error = result.stderr })
    else
        mesh.service.emit({ ok = false, error = 'invalid connection_id' })
    end
end

function on_command_disconnect()
    if not has_control() then
        mesh.service.emit({ ok = false, error = 'permission denied' })
        return
    end
    local payload = mesh.service.payload()
    local id = payload.connection_id or payload.id or ''
    if id == '' then
        mesh.service.emit({ ok = false, error = 'missing connection_id' })
        return
    end
    if is_bluetooth_mac(id) then
        local result = mesh.exec('bluetoothctl', { 'disconnect', id })
        mesh.service.emit({ ok = result.success, error = result.stderr })
    elseif is_uuid(id) then
        local result = mesh.exec('nmcli', { 'connection', 'down', 'uuid', id })
        mesh.service.emit({ ok = result.success, error = result.stderr })
    else
        mesh.service.emit({ ok = false, error = 'invalid connection_id' })
    end
end

function on_command_wifi_scan()
    mesh.exec("nmcli", { "device", "wifi", "rescan" })
    local result = mesh.exec("nmcli", { "-t", "-f", "SSID,SIGNAL,BSSID", "device", "wifi", "list" })
    if not result.success then
        mesh.service.emit_unavailable()
        return
    end
    local networks = {}
    for line in result.stdout:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local ssid     = cols[1] or ''
        local signal   = tonumber(cols[2]) or 0
        local bssid    = cols[3] or ''
        local strength = math.floor(signal + 0.5)
        table.insert(networks, { id = bssid, name = ssid, kind = 'wifi', strength = strength, active = false })
    end
    -- Update the cached networks list so the next poll re-emits it.
    last_networks = networks
    mesh.service.emit({
        available    = true,
        wifi_enabled = fetch_wifi_enabled(),
        connections  = fetch_connections() or {},
        devices      = {},
        networks     = networks,
        source_plugin = '@mesh/networkmanager',
    })
end

function on_command_set_wifi_enabled()
    if not has_control() then
        mesh.service.emit({ ok = false, error = 'permission denied' })
        return
    end
    local payload = mesh.service.payload()
    local state = payload.enabled and "on" or "off"
    local result = mesh.exec("nmcli", { "radio", "wifi", state })
    mesh.service.emit({ ok = result.success, error = result.stderr })
end
