-- PulseAudio audio backend.
-- Reads volume via pactl. Reacts to external volume changes (hardware keys,
-- other clients) through `pactl subscribe`, with polling as a fallback.

local SOUNDS_ROOT = "/usr/share/mesh/sounds/"

state = { available = false, percent = 0, muted = false }
local last_availability = nil
local last_state_snapshot = nil
local last_emitted_percent = nil
local last_emitted_muted = nil

local STREAM_SAFETY_POLL_MS = 1000
local FALLBACK_POLL_MS = 250

local function debug_info(message)
    mesh.log.info("[pulseaudio-audio] " .. message)
end

local function debug_warn(message)
    mesh.log.warn("[pulseaudio-audio] " .. message)
end

function start(self)
    -- With `pactl subscribe` active, live changes arrive on the stream and
    -- the poll path is only a safety net for missed/batched events.
    mesh.service.set_poll_interval(STREAM_SAFETY_POLL_MS)
    local subscribed = mesh.exec_stream("pactl", { "subscribe" })
    if subscribed then
        debug_info("subscribed to pactl for live volume events")
    else
        mesh.service.set_poll_interval(FALLBACK_POLL_MS)
        debug_warn("pactl subscribe unavailable; falling back to 250ms poll")
    end
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

    if last_availability ~= state.available then
        last_availability = state.available
        if state.available then
            debug_info("audio became available")
        else
            debug_warn("audio became unavailable")
        end
    end

    local snapshot = string.format(
        "available=%s percent=%s muted=%s",
        tostring(state.available),
        tostring(state.percent),
        tostring(state.muted)
    )
    if snapshot ~= last_state_snapshot then
        last_state_snapshot = snapshot
        debug_info("state " .. snapshot)
    end
end

local function emit_volume_changed(self)
    if state.available then
        self.VolumeChanged:fire({
            device_id = "default",
            level = state.percent,
        })
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

function on_poll(self)
    refresh_state()
end

local function maybe_emit_volume_changed(self)
    if not state.available then
        return
    end
    if last_emitted_percent == state.percent and last_emitted_muted == state.muted then
        return
    end
    last_emitted_percent = state.percent
    last_emitted_muted = state.muted
    emit_volume_changed(self)
end

function on_stream_batch(self, program, lines)
    if program ~= "pactl" then
        return
    end
    local relevant = false
    for _, line in ipairs(lines) do
        if line:match("sink") or line:match("server") then
            relevant = true
            break
        end
    end
    if not relevant then
        return
    end
    refresh_state()
    maybe_emit_volume_changed(self)
end

function on_command_volume_up(self)
    debug_info("received volume_up()")
    local volume = mesh.exec("pactl", { "set-sink-volume", "@DEFAULT_SINK@", "+5%" })
    local mute = mesh.exec("pactl", { "set-sink-mute", "@DEFAULT_SINK@", "0" })
    refresh_state()
    emit_volume_changed(self)
    return { ok = volume.success and mute.success, error = volume.stderr ~= "" and volume.stderr or mute.stderr }
end

function on_command_volume_down(self)
    debug_info("received volume_down()")
    local result = mesh.exec("pactl", { "set-sink-volume", "@DEFAULT_SINK@", "-5%" })
    refresh_state()
    emit_volume_changed(self)
    return { ok = result.success, error = result.stderr }
end

function on_command_set_volume(self)
    local payload = mesh.service.payload()
    local percent = command_percent_from_payload(payload)
    debug_info("received set_volume payload volume=" .. tostring(payload.volume) .. " percent=" .. tostring(percent))
    local result = mesh.exec("pactl", { "set-sink-volume", "@DEFAULT_SINK@", tostring(percent) .. "%" })
    refresh_state()
    emit_volume_changed(self)
    return { ok = result.success, error = result.stderr }
end

function on_command_toggle_mute(self)
    debug_info("received toggle_mute()")
    local result = mesh.exec("pactl", { "set-sink-mute", "@DEFAULT_SINK@", "toggle" })
    refresh_state()
    emit_volume_changed(self)
    return { ok = result.success, error = result.stderr }
end

function on_command_set_muted(self)
    local payload = mesh.service.payload()
    local muted = payload.muted == true
    debug_info("received set_muted(" .. tostring(muted) .. ")")
    local result = mesh.exec("pactl", { "set-sink-mute", "@DEFAULT_SINK@", muted and "1" or "0" })
    refresh_state()
    emit_volume_changed(self)
    return { ok = result.success, error = result.stderr }
end

function on_command_play_sound(self)
    local payload = mesh.service.payload()
    local path = resolve_sound_path(payload.path)
    if not path then
        return { ok = false, error = "invalid sound path" }
    end
    local result = mesh.exec("aplay", { path })
    return { ok = result.success, error = result.stderr }
end
