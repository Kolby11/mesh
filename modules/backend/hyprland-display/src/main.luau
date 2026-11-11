-- Display outputs, adaptive sync, HDR and night light for Hyprland.
--
-- Hyprland's Lua config does not accept `hyprctl keyword`, so changes go
-- through `hyprctl eval`. They are runtime-only: they are persisted in
-- `self.storage` and re-applied at start.

state = {
    available = false,
    outputs = {},
    night_light = false,
    night_light_kelvin = 4500,
    adaptive_sync = false,
}

local POLL_MS = 3000

-- Hyprland's event socket (.socket2.sock) announces the changes this provider
-- shows, so it re-reads shortly after one of those events and otherwise polls
-- slowly. Without the socket it keeps the original poll.
local WATCHED_EVENTS = { monitoradded = true, monitoraddedv2 = true, monitorremoved = true, monitorremovedv2 = true, focusedmon = true, focusedmonv2 = true, configreloaded = true }
local IDLE_POLL_MS = 60000
local EVENT_DELAY_MS = 300
local event_stream = nil
local dirty = false

local function shell_value(script)
    local result = mesh.exec("sh", { "-c", script })
    local value = result.success and (result.stdout or ""):gsub("%s+$", "") or ""
    return value ~= "" and value or nil
end

local function subscribe_events()
    local sig = shell_value("printf '%s' \"$HYPRLAND_INSTANCE_SIGNATURE\"")
    if not sig then return nil end
    local dirs = {}
    local xdg = shell_value("printf '%s' \"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}\"")
    if xdg then dirs[#dirs + 1] = xdg .. "/hypr/" .. sig end
    dirs[#dirs + 1] = "/tmp/hypr/" .. sig
    for _, dir in dirs do
        local socket = dir .. "/.socket2.sock"
        local started = mesh.socket_stream(socket)
        if started then
            return type(started) == "table" and started.program or ("socket:" .. socket)
        end
    end
    return nil
end

local function settled_poll_ms()
    return event_stream and IDLE_POLL_MS or POLL_MS
end

function on_stream_batch(self, program, lines)
    if event_stream == nil or program ~= event_stream or dirty then return end
    for _, line in lines do
        if WATCHED_EVENTS[line:match("^(%w+)>>") or ""] then
            dirty = true
            mesh.service.set_poll_interval(EVENT_DELAY_MS)
            return
        end
    end
end

function on_stream_closed(self, program, _success)
    if event_stream ~= nil and program == event_stream then
        event_stream = nil
        dirty = false
        mesh.service.set_poll_interval(POLL_MS)
    end
end
local IDENTITY_KELVIN = 6000
local DEFAULT_NIGHT_KELVIN = 4500

local function log_warn(message)
    mesh.log.warn("[hyprland-display] " .. message)
end

local function safe_name(name)
    return type(name) == "string" and name:match("^[%w%-%._:]+$") ~= nil
end

local function eval_ok(code)
    local result = mesh.exec("hyprctl", { "eval", code })
    if not result.success or not (result.stdout or ""):find("ok", 1, true) then
        log_warn("hyprctl eval failed: " .. (result.stderr or result.stdout or "unknown error"))
        return false
    end
    return true
end

local function read_outputs()
    local result = mesh.exec("hyprctl", { "monitors", "-j" })
    if not result.success then return nil end
    local ok, list = pcall(mesh.json.decode, result.stdout or "")
    if not ok or type(list) ~= "table" then return nil end
    local outputs, vrr = {}, false
    for _, m in ipairs(list) do
        local preset = tostring(m.colorManagementPreset or "")
        local modes = {}
        for _, mode in ipairs(m.availableModes or {}) do modes[#modes + 1] = tostring(mode) end
        outputs[#outputs + 1] = {
            name = tostring(m.name or ""),
            description = tostring(m.description or m.name or ""),
            width = math.floor(tonumber(m.width) or 0),
            height = math.floor(tonumber(m.height) or 0),
            refresh = tonumber(m.refreshRate) or 0,
            scale = tonumber(m.scale) or 1,
            x = math.floor(tonumber(m.x) or 0),
            y = math.floor(tonumber(m.y) or 0),
            focused = m.focused == true,
            vrr = m.vrr == true,
            hdr = preset == "hdr" or preset == "hdredid",
            modes = modes,
        }
        if m.vrr == true then vrr = true end
    end
    return outputs, vrr
end

local function read_kelvin()
    local result = mesh.exec("hyprctl", { "hyprsunset", "temperature" })
    if not result.success then return nil end
    return tonumber((result.stdout or ""):match("%d+"))
end

local function refresh()
    local outputs, vrr = read_outputs()
    local kelvin = read_kelvin()
    state = {
        available = outputs ~= nil and #outputs > 0,
        outputs = outputs or {},
        adaptive_sync = vrr or false,
        night_light = kelvin ~= nil and kelvin < IDENTITY_KELVIN,
        night_light_kelvin = kelvin or state.night_light_kelvin,
    }
end

local function find_output(name)
    for _, output in ipairs(state.outputs) do
        if output.name == name then return output end
    end
    return nil
end

local function monitor_code(output, scale, hdr, mode)
    return string.format(
        'hl.monitor({ output = "%s", mode = "%s", position = "%dx%d", scale = %s, cm = "%s" })',
        output.name, mode or string.format("%dx%d@%d", output.width, output.height, math.floor(output.refresh + 0.5)),
        output.x, output.y, tostring(scale), hdr and "hdr" or "auto")
end

local function apply_vrr(enabled)
    return eval_ok(string.format("hl.config({ misc = { vrr = %d } })", enabled and 1 or 0))
end

local function apply_night(enabled, kelvin)
    local result
    if enabled then
        result = mesh.exec("hyprctl", { "hyprsunset", "temperature", tostring(kelvin) })
    else
        result = mesh.exec("hyprctl", { "hyprsunset", "identity" })
    end
    if not result.success then
        log_warn("hyprsunset is not reachable: " .. (result.stderr or "unknown error"))
    end
    return result.success
end

function start(self)
    event_stream = subscribe_events()
    mesh.service.set_poll_interval(settled_poll_ms())
    refresh()
    local saved = self.storage
    if saved.adaptive_sync ~= nil and saved.adaptive_sync ~= state.adaptive_sync then
        apply_vrr(saved.adaptive_sync == true)
    end
    if saved.night_light ~= nil then
        apply_night(saved.night_light == true, tonumber(saved.night_light_kelvin) or DEFAULT_NIGHT_KELVIN)
    end
    for _, output in ipairs(state.outputs) do
        local scale = saved["scale:" .. output.name]
        local hdr = saved["hdr:" .. output.name]
        local mode = saved["mode:" .. output.name]
        if (scale ~= nil and tonumber(scale) ~= output.scale) or (hdr ~= nil and hdr ~= output.hdr) or mode ~= nil then
            eval_ok(monitor_code(output, tonumber(scale) or output.scale, hdr == true, mode))
        end
    end
    refresh()
end

function on_poll(self)
    refresh()
    if dirty then
        dirty = false
        mesh.service.set_poll_interval(settled_poll_ms())
    end
end

function on_command_set_scale(self)
    local payload = mesh.service.payload()
    local output = find_output(payload.name)
    local scale = tonumber(payload.scale)
    if not output or not safe_name(output.name) or not scale or scale < 0.5 or scale > 4 then
        return { ok = false, error = "invalid output or scale" }
    end
    local ok = eval_ok(monitor_code(output, scale, output.hdr))
    if ok then self.storage["scale:" .. output.name] = scale end
    refresh()
    return { ok = ok }
end

function on_command_set_hdr(self)
    local payload = mesh.service.payload()
    local output = find_output(payload.name)
    if not output or not safe_name(output.name) then
        return { ok = false, error = "unknown output" }
    end
    local enabled = payload.enabled == true
    local ok = eval_ok(monitor_code(output, output.scale, enabled))
    if ok then self.storage["hdr:" .. output.name] = enabled end
    refresh()
    return { ok = ok }
end

function on_command_set_adaptive_sync(self)
    local enabled = mesh.service.payload().enabled == true
    local ok = apply_vrr(enabled)
    if ok then self.storage.adaptive_sync = enabled end
    refresh()
    return { ok = ok }
end

function on_command_set_night_light(self)
    local payload = mesh.service.payload()
    local enabled = payload.enabled == true
    local kelvin = math.max(1000, math.min(6000, math.floor(tonumber(payload.kelvin) or DEFAULT_NIGHT_KELVIN)))
    local ok = apply_night(enabled, kelvin)
    if ok then
        self.storage.night_light = enabled
        self.storage.night_light_kelvin = kelvin
    end
    refresh()
    return { ok = ok }
end

function on_command_set_mode(self)
    local payload = mesh.service.payload()
    local output = find_output(payload.name)
    local width, height, hz = tostring(payload.mode or ""):match("^(%d+)x(%d+)@([%d%.]+)")
    if not output or not safe_name(output.name) or not width then
        return { ok = false, error = "invalid output or mode" }
    end
    local mode = string.format("%sx%s@%d", width, height, math.floor(tonumber(hz) + 0.5))
    local ok = eval_ok(monitor_code(output, output.scale, output.hdr, mode))
    if ok then self.storage["mode:" .. output.name] = mode end
    refresh()
    return { ok = ok }
end
