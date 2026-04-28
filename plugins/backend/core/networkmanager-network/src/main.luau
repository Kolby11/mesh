-- NetworkManager backend.
-- Polls nmcli for connections and devices; handles connect/disconnect/scan commands.

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
    local result = mesh.exec_shell('nmcli -t -f UUID,NAME,TYPE,DEVICE,STATE,SSID,SIGNAL connection show')
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
    local result = mesh.exec_shell('nmcli -t -f DEVICE,TYPE,STATE device')
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
    local connected = mesh.exec_shell('bluetoothctl devices Connected')
    if connected.success then
        for line in connected.stdout:gmatch('([^\n]+)') do
            local mac, name = line:match('Device%s+([%w:]+)%s+(.*)')
            if mac and name then
                table.insert(devices, { id = mac, name = name, kind = 'bluetooth', state = 'connected' })
                seen[mac] = true
            end
        end
    end
    local paired = mesh.exec_shell('bluetoothctl devices Paired')
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

local function has_control()
    return mesh.service.has_capability('service.network.control')
end

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
    local net_devices = fetch_net_devices() or {}
    local bt_devices = fetch_bluetooth_devices()
    for _, d in ipairs(bt_devices) do
        table.insert(net_devices, d)
    end
    mesh.service.emit({
        available = true,
        connections = conns,
        devices = net_devices,
    })
end

function on_command_connect()
    if not has_control() then
        mesh.service.emit({ ok = false, error = 'permission denied' })
        return
    end
    local payload = mesh.service.payload()
    local id = payload.id or ''
    if id:match('^[%x:]+$') and #id == 17 then
        local result = mesh.exec_shell('bluetoothctl connect ' .. id)
        mesh.service.emit({ ok = result.success, error = result.stderr })
    else
        local result = mesh.exec_shell('nmcli connection up uuid ' .. id)
        mesh.service.emit({ ok = result.success, error = result.stderr })
    end
end

function on_command_disconnect()
    if not has_control() then
        mesh.service.emit({ ok = false, error = 'permission denied' })
        return
    end
    local payload = mesh.service.payload()
    local id = payload.id or ''
    if id:match('^[%x:]+$') and #id == 17 then
        local result = mesh.exec_shell('bluetoothctl disconnect ' .. id)
        mesh.service.emit({ ok = result.success, error = result.stderr })
    else
        local result = mesh.exec_shell('nmcli connection down uuid ' .. id)
        mesh.service.emit({ ok = result.success, error = result.stderr })
    end
end

function on_command_wifi_scan()
    mesh.exec_shell('nmcli device wifi rescan')
    local result = mesh.exec_shell('nmcli -t -f SSID,SIGNAL,BSSID device wifi list')
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
    mesh.service.emit({ available = true, networks = networks })
end

function on_command_set_wifi_enabled()
    if not has_control() then
        mesh.service.emit({ ok = false, error = 'permission denied' })
        return
    end
    local payload = mesh.service.payload()
    local cmd = payload.enabled and 'nmcli radio wifi on' or 'nmcli radio wifi off'
    local result = mesh.exec_shell(cmd)
    mesh.service.emit({ ok = result.success, error = result.stderr })
end
