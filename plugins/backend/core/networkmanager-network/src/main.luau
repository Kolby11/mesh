-- NetworkManager backend placeholder.
-- This keeps the plugin loadable while the real NetworkManager integration
-- is still being implemented in Lua.

mesh.service.set_poll_interval(15000)

-- Helpers
local function split_line(line, sep)
    sep = sep or ":"
    local res = {}
    if line == nil then return res end
    for part in (line .. sep):gmatch("(.-)" .. sep) do
        table.insert(res, part)
    end
    return res
end

-- Query nmcli for connections
local function fetch_connections()
    mesh.exec_shell('nmcli -t -f UUID,NAME,TYPE,DEVICE,STATE,SSID,SIGNAL connection show')
    if not __exec_success then return nil end
    local out = __exec_stdout
    local parts = {}
    for line in out:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local id = cols[1] or ''
        local name = cols[2] or ''
        local kind = cols[3] or ''
        local device = cols[4] or ''
        local state = cols[5] or ''
        local ssid = cols[6] or ''
        local signal = tonumber(cols[7]) or 0
        local strength = 0
        if signal and signal > 0 then strength = math.floor(signal + 0.5) end
        local active = (state == 'activated' or state == 'connected' or device ~= '')
        table.insert(parts, string.format('{"id":"%s","name":"%s","kind":"%s","strength":%d,"active":%s}',
            id, name, kind, strength, tostring(active)))
    end
    return '[' .. table.concat(parts, ',') .. ']'
end

-- Query nmcli for devices
local function fetch_devices()
    mesh.exec_shell('nmcli -t -f DEVICE,TYPE,STATE device')
    if not __exec_success then return nil end
    local out = __exec_stdout
    local parts = {}
    for line in out:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local id = cols[1] or ''
        local kind = cols[2] or ''
        local state = cols[3] or ''
        table.insert(parts, string.format('{"id":"%s","name":"%s","kind":"%s","state":"%s"}', id, id, kind, state))
    end
    if #parts == 0 then return '[]' end
    return '[' .. table.concat(parts, ',') .. ']'
end

-- Query bluetoothctl for Bluetooth devices
local function fetch_bluetooth_devices()
    mesh.exec_shell('bluetoothctl devices Connected')
    if not __exec_success then return {} end
    local out = __exec_stdout
    local parts = {}
    for line in out:gmatch('([^\n]+)') do
        -- bluetoothctl devices output: "Device <MAC> <Name>"
        local mac, name = line:match('Device%s+([%w:]+)%s+(.*)')
        if mac and name then
            table.insert(parts, string.format('{"id":"%s","name":"%s","kind":"bluetooth","state":"connected"}', mac, name))
        end
    end
    -- Also get available/paired devices
    mesh.exec_shell('bluetoothctl devices Paired')
    if __exec_success then
        local paired_out = __exec_stdout
        local seen = {}
        for _, device_str in ipairs(parts) do 
            local mac = device_str:match('"id":"([^"]+)"')
            if mac then seen[mac] = true end
        end
        for line in paired_out:gmatch('([^\n]+)') do
            local mac, name = line:match('Device%s+([%w:]+)%s+(.*)')
            if mac and name and not seen[mac] then
                table.insert(parts, string.format('{"id":"%s","name":"%s","kind":"bluetooth","state":"paired"}', mac, name))
                seen[mac] = true
            end
        end
    end
    return parts
end

-- Active connection
local function fetch_active_connection()
    mesh.exec_shell('nmcli -t -f UUID,NAME,TYPE,DEVICE,STATE connection show --active')
    if not __exec_success then return nil end
    local out = __exec_stdout
    local line = out:match('([^\n]+)')
    if not line or line == '' then return 'null' end
    local cols = split_line(line, ':')
    if #cols < 3 then cols = split_line(line, '|') end
    local id = cols[1] or ''
    local name = cols[2] or ''
    local kind = cols[3] or ''
    local strength = 0
    local active = true
    return string.format('{"id":"%s","name":"%s","kind":"%s","strength":%d,"active":%s}', id, name, kind, strength, tostring(active))
end

-- Bluetooth connection helper: connect to a paired Bluetooth device
local function connect_bluetooth(device_id)
    mesh.exec_shell('bluetoothctl connect ' .. device_id)
    if __exec_success then
        return result_ok()
    else
        return result_err(__exec_stderr or 'failed to connect bluetooth device')
    end
end

-- Bluetooth disconnection helper: disconnect from a Bluetooth device
local function disconnect_bluetooth(device_id)
    mesh.exec_shell('bluetoothctl disconnect ' .. device_id)
    if __exec_success then
        return result_ok()
    else
        return result_err(__exec_stderr or 'failed to disconnect bluetooth device')
    end
end

local function result_ok()
    return '{"ok":true}'
end

local function result_err(msg)
    return string.format('{"ok":false,"error":"%s"}', msg or 'failed')
end

local function has_control()
    if mesh.service and mesh.service.has_capability then
        return mesh.service.has_capability('service.network.control')
    end
    return false
end

-- Methods exposed to the interface
function connections()
    local out = fetch_connections()
    if out then
        mesh.exec_shell(('printf "%s"'):format(out))
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function devices()
    mesh.exec_shell('nmcli -t -f DEVICE,TYPE,STATE device')
    if not __exec_success then mesh.service.emit_unavailable(); return end
    local out = __exec_stdout
    local net_parts = {}
    for line in out:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local id = cols[1] or ''
        local kind = cols[2] or ''
        local state = cols[3] or ''
        table.insert(net_parts, string.format('{"id":"%s","name":"%s","kind":"%s","state":"%s"}', id, id, kind, state))
    end
    
    -- Add Bluetooth devices
    local bt_parts = fetch_bluetooth_devices()
    local all_parts = net_parts
    for _, device in ipairs(bt_parts) do
        table.insert(all_parts, device)
    end
    
    local json = '[' .. table.concat(all_parts, ',') .. ']'
    mesh.exec_shell(('printf "%s"'):format(json))
    mesh.service.emit_json()
end

function active_connection()
    local out = fetch_active_connection()
    if out then
        if out == 'null' then
            mesh.exec_shell('printf "null"')
        else
            mesh.exec_shell(('printf "%s"'):format(out))
        end
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function connect(connection_id)
    if not has_control() then
        mesh.exec_shell(('printf "%s"'):format('{"ok":false,"error":"permission denied"}'))
        mesh.service.emit_json()
        return
    end
    -- Check if it's a Bluetooth device (format: xx:xx:xx:xx:xx:xx) or a network connection (UUID)
    if connection_id:match('^[%x:]+$') and #connection_id == 17 then
        -- Bluetooth device MAC address
        local result = connect_bluetooth(connection_id)
        mesh.exec_shell(('printf "%s"'):format(result))
        mesh.service.emit_json()
    else
        -- Network connection UUID
        mesh.exec_shell('nmcli connection up uuid ' .. connection_id)
        if __exec_success then
            mesh.exec_shell(('printf "%s"'):format(result_ok()))
            mesh.service.emit_json()
            mesh.service.emit_event('ConnectionChanged', '{"connection":' .. (fetch_active_connection() or 'null') .. '}')
        else
            mesh.exec_shell(('printf "%s"'):format(result_err(__exec_stderr or 'failed')))
            mesh.service.emit_json()
        end
    end
end

function disconnect(connection_id)
    if not has_control() then
        mesh.exec_shell(('printf "%s"'):format('{"ok":false,"error":"permission denied"}'))
        mesh.service.emit_json()
        return
    end
    -- Check if it's a Bluetooth device (format: xx:xx:xx:xx:xx:xx) or a network connection (UUID)
    if connection_id:match('^[%x:]+$') and #connection_id == 17 then
        -- Bluetooth device MAC address
        local result = disconnect_bluetooth(connection_id)
        mesh.exec_shell(('printf "%s"'):format(result))
        mesh.service.emit_json()
    else
        -- Network connection UUID
        mesh.exec_shell('nmcli connection down uuid ' .. connection_id)
        if __exec_success then
            mesh.exec_shell(('printf "%s"'):format(result_ok()))
            mesh.service.emit_json()
            mesh.service.emit_event('ConnectionChanged', '{"connection":null}')
        else
            mesh.exec_shell(('printf "%s"'):format(result_err(__exec_stderr or 'failed')))
            mesh.service.emit_json()
        end
    end
end

function wifi_scan()
    mesh.exec_shell('nmcli device wifi rescan')
    mesh.exec_shell('nmcli -t -f SSID,SIGNAL,BSSID device wifi list')
    if not __exec_success then mesh.service.emit_unavailable(); return end
    local out = __exec_stdout
    local parts = {}
    for line in out:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local ssid = cols[1] or ''
        local signal = tonumber(cols[2]) or 0
        local bssid = cols[3] or ''
        local strength = math.floor(signal + 0.5)
        table.insert(parts, string.format('{"id":"%s","name":"%s","kind":"wifi","strength":%d,"active":false}', bssid, ssid, strength))
    end
    local json = '[' .. table.concat(parts, ',') .. ']'
    mesh.exec_shell(('printf "%s"'):format(json))
    mesh.service.emit_json()
end

function set_wifi_enabled(enabled)
    if not has_control() then
        mesh.exec_shell(('printf "%s"'):format('{"ok":false,"error":"permission denied"}'))
        mesh.service.emit_json()
        return
    end
    local cmd = 'nmcli radio wifi '
    if enabled then cmd = cmd .. 'on' else cmd = cmd .. 'off' end
    mesh.exec_shell(cmd)
    if __exec_success then
        mesh.exec_shell(('printf "%s"'):format(result_ok()))
        mesh.service.emit_json()
    else
        mesh.exec_shell(('printf "%s"'):format(result_err(__exec_stderr or 'failed')))
        mesh.service.emit_json()
    end
end

-- Poll handler: emit combined connections and devices (network + Bluetooth)
local warned = false
function on_poll()
    local conns = fetch_connections()
    if not conns then
        if not warned then mesh.log.warn('NetworkManager backend failed to query system (nmcli) or nmcli missing') warned = true end
        mesh.service.emit_unavailable()
        return
    end
    
    -- Gather network devices
    mesh.exec_shell('nmcli -t -f DEVICE,TYPE,STATE device')
    if not __exec_success then
        if not warned then mesh.log.warn('NetworkManager backend failed to query devices') warned = true end
        mesh.service.emit_unavailable()
        return
    end
    local net_out = __exec_stdout
    local net_parts = {}
    for line in net_out:gmatch('([^\n]+)') do
        local cols = split_line(line, ':')
        if #cols < 3 then cols = split_line(line, '|') end
        local id = cols[1] or ''
        local kind = cols[2] or ''
        local state = cols[3] or ''
        table.insert(net_parts, string.format('{"id":"%s","name":"%s","kind":"%s","state":"%s"}', id, id, kind, state))
    end
    
    -- Gather Bluetooth devices
    local bt_parts = fetch_bluetooth_devices()
    
    -- Combine all devices
    local all_parts = net_parts
    for _, device in ipairs(bt_parts) do
        table.insert(all_parts, device)
    end
    local devices_json = '[' .. table.concat(all_parts, ',') .. ']'
    
    local payload = string.format('{"connections":%s,"devices":%s}', conns, devices_json)
    mesh.exec_shell(('printf "%s"'):format(payload))
    mesh.service.emit_json()
end
