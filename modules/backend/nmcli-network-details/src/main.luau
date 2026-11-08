-- Wi-Fi details, saved connections, VPN and hotspot through nmcli.
--
-- Terse output is read with `-e yes`, so `:` inside a field arrives as `\:`.
-- Names that reach nmcli as arguments are rejected when they could be read
-- as options.

state = {
    available = false,
    connection = {
        kind = "none", name = "", ssid = "", bssid = "", frequency_mhz = 0, channel = 0, band = "", security = "",
        signal = 0, rate_mbps = 0, interface = "", mac = "", ip4 = "", gateway = "", dns = "", ip6 = "", mtu = 0, speed_mbps = 0,
    },
    networks = {},
    saved = {},
    hotspot_active = false,
}

local POLL_MS = 10000
local HOTSPOT = "Hotspot"
local SAVED_KINDS = {
    ["802-11-wireless"] = "wifi", ["802-3-ethernet"] = "ethernet", vpn = "vpn", wireguard = "vpn",
}

local function log_warn(message)
    mesh.log.warn("[nmcli-network-details] " .. message)
end

local function split_terse(line)
    local fields, current, i = {}, {}, 1
    while i <= #line do
        local c = line:sub(i, i)
        if c == "\\" then
            current[#current + 1] = line:sub(i + 1, i + 1)
            i = i + 2
        elseif c == ":" then
            fields[#fields + 1] = table.concat(current)
            current = {}
            i = i + 1
        else
            current[#current + 1] = c
            i = i + 1
        end
    end
    fields[#fields + 1] = table.concat(current)
    return fields
end

local function nmcli(args)
    return mesh.exec("nmcli", args)
end

local function safe_arg(value)
    return type(value) == "string" and value ~= "" and value:sub(1, 1) ~= "-" and #value <= 128
end

local function band_of(mhz)
    if mhz <= 0 then return "" end
    if mhz < 3000 then return "2.4 GHz" end
    if mhz < 5925 then return "5 GHz" end
    return "6 GHz"
end

local function security_of(raw)
    raw = raw or ""
    if raw == "" or raw == "--" then return "Open" end
    if raw:find("WPA3", 1, true) then return "WPA3" end
    if raw:find("WPA2", 1, true) then return "WPA2" end
    if raw:find("WPA1", 1, true) then return "WPA" end
    if raw:find("WEP", 1, true) then return "WEP" end
    return raw
end

local function number_of(text)
    return math.floor(tonumber((text or ""):match("[%d%.]+")) or 0)
end

local function read_saved()
    local saved, by_name = {}, {}
    local result = nmcli({ "-t", "-e", "yes", "-f", "NAME,UUID,TYPE,AUTOCONNECT,ACTIVE", "connection", "show" })
    if not result.success then return saved, by_name end
    for line in (result.stdout or ""):gmatch("[^\n]+") do
        local f = split_terse(line)
        local kind = SAVED_KINDS[f[3] or ""]
        if kind and f[1] ~= HOTSPOT then
            local entry = { name = f[1], uuid = f[2], kind = kind, autoconnect = f[4] == "yes", active = f[5] == "yes" }
            saved[#saved + 1] = entry
            by_name[entry.name] = entry
        end
    end
    table.sort(saved, function(a, b)
        if a.active ~= b.active then return a.active end
        return a.name:lower() < b.name:lower()
    end)
    return saved, by_name
end

local function read_networks(by_name)
    local networks, seen = {}, {}
    local result = nmcli({ "-t", "-e", "yes", "-f", "IN-USE,SSID,BSSID,CHAN,FREQ,RATE,SIGNAL,SECURITY", "device", "wifi", "list" })
    if not result.success then return networks end
    for line in (result.stdout or ""):gmatch("[^\n]+") do
        local f = split_terse(line)
        local ssid = f[2] or ""
        if ssid ~= "" then
            local mhz = number_of(f[5])
            local entry = {
                ssid = ssid, bssid = f[3] or "", frequency_mhz = mhz, channel = number_of(f[4]), band = band_of(mhz),
                security = security_of(f[8]), signal = number_of(f[7]), rate_mbps = number_of(f[6]),
                saved = by_name[ssid] ~= nil, active = f[1] == "*",
            }
            local index = seen[ssid]
            if not index then
                networks[#networks + 1] = entry
                seen[ssid] = #networks
            elseif entry.active or (not networks[index].active and entry.signal > networks[index].signal) then
                networks[index] = entry
            end
        end
    end
    table.sort(networks, function(a, b)
        if a.active ~= b.active then return a.active end
        if a.signal ~= b.signal then return a.signal > b.signal end
        return a.ssid < b.ssid
    end)
    return networks
end

local function device_status()
    local devices = {}
    local result = nmcli({ "-t", "-f", "DEVICE,TYPE,STATE", "device", "status" })
    if not result.success then return devices end
    for line in (result.stdout or ""):gmatch("[^\n]+") do
        local f = split_terse(line)
        devices[#devices + 1] = { device = f[1], kind = f[2], state = f[3] }
    end
    return devices
end

local function read_connection(networks, hotspot_device)
    local empty = {
        kind = "none", name = "", ssid = "", bssid = "", frequency_mhz = 0, channel = 0, band = "", security = "",
        signal = 0, rate_mbps = 0, interface = "", mac = "", ip4 = "", gateway = "", dns = "", ip6 = "", mtu = 0, speed_mbps = 0,
    }
    local chosen = nil
    for _, d in ipairs(device_status()) do
        if d.state == "connected" and (d.kind == "ethernet" or d.kind == "wifi") then
            if d.device ~= hotspot_device and (chosen == nil or d.kind == "ethernet") then chosen = d end
        end
    end
    if not chosen then return empty end
    local shown = mesh.exec("nmcli", { "-t", "-e", "no", "-f", "GENERAL.HWADDR,GENERAL.MTU,GENERAL.CONNECTION,IP4.ADDRESS,IP4.GATEWAY,IP4.DNS,IP6.ADDRESS,CAPABILITIES.SPEED", "device", "show", chosen.device })
    if not shown.success then return empty end
    local values, dns = {}, {}
    for line in (shown.stdout or ""):gmatch("[^\n]+") do
        local key, value = line:match("^([^:]+):(.*)$")
        if key then
            local base = key:gsub("%[%d+%]$", "")
            if base == "IP4.DNS" then dns[#dns + 1] = value
            elseif values[base] == nil then values[base] = value end
        end
    end
    local connection = {
        kind = chosen.kind, name = values["GENERAL.CONNECTION"] or "", ssid = "", bssid = "", frequency_mhz = 0, channel = 0, band = "",
        security = "", signal = 0, rate_mbps = 0, interface = chosen.device, mac = values["GENERAL.HWADDR"] or "",
        ip4 = values["IP4.ADDRESS"] or "", gateway = values["IP4.GATEWAY"] or "", dns = table.concat(dns, ", "),
        ip6 = values["IP6.ADDRESS"] or "", mtu = number_of(values["GENERAL.MTU"]), speed_mbps = number_of(values["CAPABILITIES.SPEED"]),
    }
    if chosen.kind == "wifi" then
        for _, n in ipairs(networks) do
            if n.active then
                connection.ssid, connection.bssid, connection.frequency_mhz, connection.channel = n.ssid, n.bssid, n.frequency_mhz, n.channel
                connection.band, connection.security, connection.signal, connection.rate_mbps = n.band, n.security, n.signal, n.rate_mbps
            end
        end
    end
    return connection
end

local function refresh()
    local saved, by_name = read_saved()
    local networks = read_networks(by_name)
    local hotspot_device = nil
    local hotspot_active = false
    local listing = nmcli({ "-t", "-e", "yes", "-f", "NAME,DEVICE,ACTIVE", "connection", "show", "--active" })
    if listing.success then
        for line in (listing.stdout or ""):gmatch("[^\n]+") do
            local f = split_terse(line)
            if f[1] == HOTSPOT then hotspot_active = true; hotspot_device = f[2] end
        end
    end
    state = {
        available = true,
        connection = read_connection(networks, hotspot_device),
        networks = networks,
        saved = saved,
        hotspot_active = hotspot_active,
    }
end

local function finish(result)
    if not result.success then
        log_warn("nmcli failed: " .. (result.stderr or result.stdout or "unknown error"))
    end
    refresh()
    return { ok = result.success, error = result.stderr }
end

function start(self)
    mesh.service.set_poll_interval(POLL_MS)
    refresh()
end

function on_poll(self)
    refresh()
end

function on_command_refresh(self)
    nmcli({ "device", "wifi", "rescan" })
    refresh()
    return { ok = true }
end

function on_command_connect(self)
    local payload = mesh.service.payload()
    local ssid = payload.ssid
    if not safe_arg(ssid) then return { ok = false, error = "invalid network name" } end
    local password = tostring(payload.password or "")
    if password ~= "" then
        if password:sub(1, 1) == "-" then return { ok = false, error = "invalid password" } end
        return finish(nmcli({ "device", "wifi", "connect", ssid, "password", password }))
    end
    return finish(nmcli({ "device", "wifi", "connect", ssid }))
end

function on_command_disconnect(self)
    local device = state.connection.interface
    if not safe_arg(device) then return { ok = false, error = "not connected" } end
    return finish(nmcli({ "device", "disconnect", device }))
end

function on_command_forget(self)
    local name = mesh.service.payload().name
    if not safe_arg(name) then return { ok = false, error = "invalid name" } end
    return finish(nmcli({ "connection", "delete", "id", name }))
end

function on_command_set_autoconnect(self)
    local payload = mesh.service.payload()
    if not safe_arg(payload.name) then return { ok = false, error = "invalid name" } end
    return finish(nmcli({ "connection", "modify", "id", payload.name, "connection.autoconnect", payload.enabled == true and "yes" or "no" }))
end

function on_command_connection_up(self)
    local name = mesh.service.payload().name
    if not safe_arg(name) then return { ok = false, error = "invalid name" } end
    return finish(nmcli({ "connection", "up", "id", name }))
end

function on_command_connection_down(self)
    local name = mesh.service.payload().name
    if not safe_arg(name) then return { ok = false, error = "invalid name" } end
    return finish(nmcli({ "connection", "down", "id", name }))
end

function on_command_set_hotspot(self)
    local payload = mesh.service.payload()
    if payload.enabled ~= true then
        return finish(nmcli({ "connection", "down", HOTSPOT }))
    end
    local ssid, password = payload.ssid, tostring(payload.password or "")
    if not safe_arg(ssid) or #password < 8 or password:sub(1, 1) == "-" then
        return { ok = false, error = "a name and a password of at least 8 characters are required" }
    end
    local wifi = nil
    for _, d in ipairs(device_status()) do
        if d.kind == "wifi" then wifi = d.device; break end
    end
    if not safe_arg(wifi) then return { ok = false, error = "no Wi-Fi device" } end
    return finish(nmcli({ "device", "wifi", "hotspot", "ifname", wifi, "ssid", ssid, "password", password }))
end
