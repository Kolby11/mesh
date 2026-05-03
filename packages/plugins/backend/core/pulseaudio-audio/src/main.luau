-- PulseAudio audio backend.
-- Polls volume via pactl and responds to audio service commands.

local SOUNDS_ROOT = "/usr/share/mesh/sounds/"

state = { available = false, percent = 0, muted = false }

function init()
    mesh.service.set_poll_interval(500)
end

local function read_state()
    local volume = mesh.exec("pactl", { "get-sink-volume", "@DEFAULT_SINK@" })
    local mute = mesh.exec("pactl", { "get-sink-mute", "@DEFAULT_SINK@" })
    if not volume.success or not mute.success then
        return nil
    end

    local percent = tonumber((volume.stdout or ""):match("(%d+)%%")) or 0
    local muted = (mute.stdout or ""):match("Mute:%s+yes") ~= nil
    return {
        available = true,
        percent = percent,
        muted = muted,
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
    local volume = mesh.exec("pactl", { "set-sink-volume", "@DEFAULT_SINK@", "+5%" })
    local mute = mesh.exec("pactl", { "set-sink-mute", "@DEFAULT_SINK@", "0" })
    refresh_state()
    return { ok = volume.success and mute.success, error = volume.stderr ~= "" and volume.stderr or mute.stderr }
end

function on_command_volume_down()
    local result = mesh.exec("pactl", { "set-sink-volume", "@DEFAULT_SINK@", "-5%" })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_set_volume()
    local payload = mesh.service.payload()
    local percent = command_percent_from_payload(payload)
    local result = mesh.exec("pactl", { "set-sink-volume", "@DEFAULT_SINK@", tostring(percent) .. "%" })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_toggle_mute()
    local result = mesh.exec("pactl", { "set-sink-mute", "@DEFAULT_SINK@", "toggle" })
    refresh_state()
    return { ok = result.success, error = result.stderr }
end

function on_command_set_muted()
    local payload = mesh.service.payload()
    local muted = payload.muted == true
    local result = mesh.exec("pactl", { "set-sink-mute", "@DEFAULT_SINK@", muted and "1" or "0" })
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
