-- Bluetooth through bluetoothctl.
--
-- Discovery runs as a bounded stream so a scan never blocks commands. BlueZ
-- signals mark the state dirty and the next poll re-reads it; every refresh
-- spawns one bluetoothctl per device, so an idle adapter is only re-read on
-- the slow safety poll and after commands.

state = {
    available = false,
    powered = false,
    scanning = false,
    adapter_name = "",
    adapter_address = "",
    discoverable = false,
    pairable = false,
    paired_devices = {},
    nearby_devices = {},
}

local IDLE_POLL_MS = 60000
local DIRTY_POLL_MS = 2000
local UNMONITORED_POLL_MS = 4000
local SCAN_SECONDS = "12"
local scan_stream = nil
local monitor = nil
local dirty = false

local function log_warn(message)
    mesh.log.warn("[bluetoothctl-bluetooth] " .. message)
end

local function valid_id(id)
    return type(id) == "string" and id:match("^%x%x:%x%x:%x%x:%x%x:%x%x:%x%x$") ~= nil
end

local function parse_devices(output)
    local list = {}
    for id, name in (output or ""):gmatch("Device (%x%x:%x%x:%x%x:%x%x:%x%x:%x%x) ([^\n]*)") do
        list[#list + 1] = { id = id, name = name }
    end
    return list
end

local function describe(device)
    local info = mesh.exec("bluetoothctl", { "info", device.id })
    local text = info.success and (info.stdout or "") or ""
    local icon = text:match("Icon: ([%w%-]+)") or ""
    local modalias = text:match("Modalias: ([^\n]+)") or ""
    return {
        id = device.id,
        address = device.id,
        name = device.name ~= "" and device.name or device.id,
        kind = icon,
        icon = icon,
        paired = text:match("Paired: (%a+)") == "yes",
        connected = text:match("Connected: (%a+)") == "yes",
        trusted = text:match("Trusted: (%a+)") == "yes",
        blocked = text:match("Blocked: (%a+)") == "yes",
        battery = tonumber(text:match("Battery Percentage: 0x%x+ %((%d+)%)")) or -1,
        rssi = tonumber(text:match("RSSI: [^%(]*%((%-?%d+)%)")) or 0,
        vendor_id = modalias:match("v(%x%x%x%x)") or "",
        product_id = modalias:match("p(%x%x%x%x)") or "",
    }
end

local function refresh()
    local show = mesh.exec("bluetoothctl", { "show" })
    if not show.success or not (show.stdout or ""):find("Controller", 1, true) then
        state = {
            available = false, powered = false, scanning = scan_stream ~= nil,
            adapter_name = "", adapter_address = "", discoverable = false, pairable = false,
            paired_devices = {}, nearby_devices = {},
        }
        return
    end
    local text = show.stdout or ""
    local powered = text:match("Powered: (%a+)") == "yes"
    local paired, nearby = {}, {}
    if powered then
        local paired_ids = {}
        for _, device in ipairs(parse_devices(mesh.exec("bluetoothctl", { "devices", "Paired" }).stdout)) do
            paired_ids[device.id] = true
            paired[#paired + 1] = describe(device)
        end
        for _, device in ipairs(parse_devices(mesh.exec("bluetoothctl", { "devices" }).stdout)) do
            if #nearby < 25 and not paired_ids[device.id] and device.name ~= device.id:gsub(":", "-") then
                local entry = describe(device)
                entry.paired = false
                nearby[#nearby + 1] = entry
            end
        end
    end
    state = {
        available = true,
        powered = powered,
        scanning = scan_stream ~= nil,
        adapter_name = text:match("Alias: ([^\n]*)") or text:match("Name: ([^\n]*)") or "",
        adapter_address = text:match("Controller (%x%x:%x%x:%x%x:%x%x:%x%x:%x%x)") or "",
        discoverable = text:match("Discoverable: (%a+)") == "yes",
        pairable = text:match("Pairable: (%a+)") == "yes",
        paired_devices = paired,
        nearby_devices = nearby,
    }
end

function start(self)
    mesh.service.set_poll_interval(IDLE_POLL_MS)
    -- ObjectManager announces devices from the root path, so the namespace
    -- cannot be narrowed to /org/bluez.
    monitor = mesh.dbus_subscribe("system", { sender = "org.bluez", path_namespace = "/" })
    if not monitor then mesh.service.set_poll_interval(UNMONITORED_POLL_MS) end
    refresh()
end

function on_poll(self)
    refresh()
    if dirty then
        dirty = false
        mesh.service.set_poll_interval(IDLE_POLL_MS)
    end
end

function on_stream_line(self, program, _line)
    if monitor == nil or program ~= monitor.program or dirty then
        return
    end
    dirty = true
    mesh.service.set_poll_interval(DIRTY_POLL_MS)
end

function on_stream_closed(self, program, _success)
    if monitor ~= nil and program == monitor.program then
        monitor = nil
        dirty = false
        mesh.service.set_poll_interval(UNMONITORED_POLL_MS)
    elseif program == "bluetoothctl" then
        scan_stream = nil
        refresh()
    end
end

local function run(args)
    local result = mesh.exec("bluetoothctl", args)
    if not result.success then
        log_warn("bluetoothctl " .. tostring(args[1]) .. " failed: " .. (result.stderr or result.stdout or "unknown error"))
    end
    refresh()
    return { ok = result.success, error = result.stderr }
end

function on_command_set_enabled(self)
    local enabled = mesh.service.payload().enabled == true
    return run({ "power", enabled and "on" or "off" })
end

function on_command_scan(self)
    if scan_stream ~= nil or not state.powered then
        return { ok = scan_stream ~= nil }
    end
    scan_stream = mesh.exec_stream("bluetoothctl", { "--timeout", SCAN_SECONDS, "scan", "on" })
    refresh()
    return { ok = scan_stream ~= nil }
end

function on_command_pair(self)
    local id = mesh.service.payload().id
    if not valid_id(id) then return { ok = false, error = "invalid device id" } end
    local paired = mesh.exec("bluetoothctl", { "pair", id })
    if paired.success then
        mesh.exec("bluetoothctl", { "trust", id })
        mesh.exec("bluetoothctl", { "connect", id })
    else
        log_warn("pairing failed: " .. (paired.stderr or paired.stdout or "unknown error"))
    end
    refresh()
    return { ok = paired.success, error = paired.stderr }
end

function on_command_connect(self)
    local id = mesh.service.payload().id
    if not valid_id(id) then return { ok = false, error = "invalid device id" } end
    return run({ "connect", id })
end

function on_command_disconnect(self)
    local id = mesh.service.payload().id
    if not valid_id(id) then return { ok = false, error = "invalid device id" } end
    return run({ "disconnect", id })
end

function on_command_forget(self)
    local id = mesh.service.payload().id
    if not valid_id(id) then return { ok = false, error = "invalid device id" } end
    return run({ "remove", id })
end

function on_command_set_trusted(self)
    local payload = mesh.service.payload()
    if not valid_id(payload.id) then return { ok = false, error = "invalid device id" } end
    return run({ payload.enabled == true and "trust" or "untrust", payload.id })
end

function on_command_set_discoverable(self)
    return run({ "discoverable", mesh.service.payload().enabled == true and "on" or "off" })
end

function on_command_set_pairable(self)
    return run({ "pairable", mesh.service.payload().enabled == true and "on" or "off" })
end

function on_command_set_adapter_name(self)
    local name = tostring(mesh.service.payload().name or "")
    if name == "" or #name > 64 or name:sub(1, 1) == "-" then return { ok = false, error = "invalid name" } end
    return run({ "system-alias", name })
end
