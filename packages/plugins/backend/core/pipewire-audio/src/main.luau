-- PipeWire audio backend.
-- Polls volume via wpctl and responds to audio service commands.

local SOUNDS_ROOT = "/usr/share/mesh/sounds/"

state = { available = false, percent = 0, muted = false }

function init()
    mesh.service.set_poll_interval(500)
end

local function resolve_sink_id()
    local result = mesh.exec("wpctl", { "status" })
    if not result.success then
        return nil
    end

    local first = nil
    local in_sinks = false
    for line in (result.stdout or ""):gmatch("([^\n]+)") do
        if line:match("Sinks:") then
            in_sinks = true
        elseif in_sinks and line:match("Sources:") then
            in_sinks = false
        elseif in_sinks then
            local selected = line:match("%*%s*(%d+)%.")
            if selected then
                return selected
            end
            first = first or line:match("(%d+)%.")
        end
    end

    return first
end

local function read_state()
    local sink_id = resolve_sink_id()
    if sink_id == nil then
        return nil
    end

    local result = mesh.exec("wpctl", { "get-volume", sink_id })
    if not result.success then
        return nil
    end

    local raw = result.stdout or ""
    local level = tonumber(raw:match("(%d+%.%d+)")) or 0
    return {
        available = true,
        percent = math.floor((level * 100) + 0.5),
        muted = raw:match("%[MUTED%]") ~= nil,
    }
end

local function refresh_state()
    local next_state = read_state()
    if next_state then
        state = next_state
    else
        state = { available = false, percent = 0, muted = false }
    end
end

local function clamp_percent(percent)
    percent = tonumber(percent) or 0
    return math.max(0, math.min(100, percent))
end

local function command_percent_from_payload(payload)
    if payload.percent ~= nil then
        return clamp_percent(payload.percent)
    end

    return clamp_percent(math.floor(((tonumber(payload.volume) or 0) * 100) + 0.5))
end

local function resolve_sound_path(path)
    if type(path) ~= "string" or path == "" then
        return nil
    end
    if path:find("..", 1, true) then
        return nil
    end
    if path:sub(1, #SOUNDS_ROOT) ~= SOUNDS_ROOT then
        return nil
    end
    return path
end

function on_poll()
    refresh_state()
end

function on_command_volume_up()
    local sink_id = resolve_sink_id()
    if not sink_id then
        refresh_state()
        return { ok = false, error = "audio sink unavailable" }
    end
    local volume = mesh.exec("wpctl", { "set-volume", sink_id, "5%+" })
    local mute = mesh.exec("wpctl", { "set-mute", sink_id, "0" })
    refresh_state()
    return { ok = volume.success and mute.success, error = volume.stderr ~= "" and volume.stderr or mute.stderr }
end

function on_command_volume_down()
    local sink_id = resolve_sink_id()
    if not sink_id then
        refresh_state()
        return { ok = false, error = "audio sink unavailable" }
    end
    local result = mesh.exec("wpctl", { "set-volume", sink_id, "5%-" })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_set_volume()
    local payload = mesh.service.payload()
    local percent = command_percent_from_payload(payload)
    local sink_id = resolve_sink_id()
    if not sink_id then
        refresh_state()
        return { ok = false, error = "audio sink unavailable" }
    end
    local result = mesh.exec("wpctl", { "set-volume", sink_id, tostring(percent) .. "%" })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_toggle_mute()
    local sink_id = resolve_sink_id()
    if not sink_id then
        refresh_state()
        return { ok = false, error = "audio sink unavailable" }
    end
    local result = mesh.exec("wpctl", { "set-mute", sink_id, "toggle" })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_set_muted()
    local payload = mesh.service.payload()
    local muted = payload.muted == true
    local sink_id = resolve_sink_id()
    if not sink_id then
        refresh_state()
        return { ok = false, error = "audio sink unavailable" }
    end
    local result = mesh.exec("wpctl", { "set-mute", sink_id, muted and "1" or "0" })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_play_sound()
    local payload = mesh.service.payload()
    local path = resolve_sound_path(payload.path)
    if not path then
        return { ok = false, error = "invalid sound path" }
    end
    local result = mesh.exec("aplay", { path })
    return { ok = result.success, error = result.stderr }
end
