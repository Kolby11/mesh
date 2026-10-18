-- NetworkManager provider.
--
-- `nmcli monitor` prints a line for every connectivity, device or connection
-- change; each line marks the state dirty and the next poll tick (kept short
-- while dirty) re-reads it, so a burst of monitor lines costs one refresh.

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

local function refresh()
    state = read_state() or {
        available = false, connectivity = "none", kind = "none", wifi_enabled = false,
        ssid = "", signal = 0, interface_name = "", networks = {},
    }
end

function start(self)
    mesh.service.set_poll_interval(IDLE_POLL_MS)
    if not mesh.exec_stream("nmcli", { "monitor" }) then
        mesh.service.set_poll_interval(5000)
    end
    refresh()
end

function on_stream_line(self, program, _line)
    if program ~= "nmcli" or dirty then
        return
    end
    dirty = true
    mesh.service.set_poll_interval(DIRTY_POLL_MS)
end

function on_stream_closed(self, program, _success)
    if program == "nmcli" then
        mesh.service.set_poll_interval(5000)
    end
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
