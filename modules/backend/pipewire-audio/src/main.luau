-- PipeWire audio backend.
--
-- Reads volume via wpctl. Reacts to external volume changes (hardware keys,
-- other clients) through `pw-mon`, streamed via mesh.exec_stream. A slow
-- safety-net poll covers changes that produce no monitor output.

local SOUNDS_ROOT = "/usr/share/mesh/sounds/"

state = { available = false, percent = 0, muted = false }
local last_availability = nil
local last_state_snapshot = nil
local last_emitted_percent = nil
local last_emitted_muted = nil

local function debug_info(message)
    mesh.log.info("[pipewire-audio] " .. message)
end

local function debug_warn(message)
    mesh.log.warn("[pipewire-audio] " .. message)
end

-- Every wpctl invocation registers a short-lived PipeWire client, which
-- pw-mon reports as a Client added/removed batch. Without accounting, each
-- refresh would spawn wpctl, whose connect event would arrive as a new
-- pw-mon batch and trigger another refresh — a self-sustaining exec loop
-- (~22 wpctl spawns/sec measured). This counter tracks connects we caused
-- ourselves so on_stream_batch can swallow them.
local pending_self_noise = 0
local stream_active = false

local function wpctl(args)
    if stream_active then
        pending_self_noise = pending_self_noise + 1
    end
    return mesh.exec("wpctl", args)
end

function start(self)
    -- Slow safety-net poll. With pw-mon active, external volume changes are
    -- detected through client/object events (e.g. the compositor's wpctl run
    -- for a hardware volume key registers a client), so the poll only covers
    -- changes that produce no monitor output at all.
    mesh.service.set_poll_interval(1000)

    local started = mesh.exec_stream(
        "pw-mon",
        { "--hide-props", "--hide-params", "--print-separator" }
    )
    if started then
        stream_active = true
        debug_info("subscribed to pw-mon for live volume events")
    else
        mesh.service.set_poll_interval(250)
        debug_warn("pw-mon stream unavailable; falling back to 250ms poll")
    end
end

local function resolve_sink_id()
    local result = wpctl({ "status" })
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
    -- @DEFAULT_AUDIO_SINK@ resolves server-side, avoiding a second wpctl
    -- status call. Each saved wpctl invocation is ~20ms, which matters at
    -- key-repeat rates (~30Hz) where every refresh is on the hot path.
    local result = wpctl({ "get-volume", "@DEFAULT_AUDIO_SINK@" })
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

local function wpctl_with_sink_fallback(args)
    local result = wpctl(args)
    if result.success or not (result.stderr and result.stderr:find("not a valid ID", 1, true)) then
        return result
    end
    local sink_id = resolve_sink_id()
    if not sink_id then
        return result
    end
    debug_info("@DEFAULT_AUDIO_SINK@ alias failed; retrying with resolved sink id=" .. sink_id)
    local fallback_args = {}
    for _, arg in ipairs(args) do
        table.insert(fallback_args, arg == "@DEFAULT_AUDIO_SINK@" and sink_id or arg)
    end
    return wpctl(fallback_args)
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

-- Scan one pw-mon batch. Returns whether it carries a real signal (any
-- `changed:` block, or an `added:` block whose object is not a Client) and
-- how many Client `added:` blocks it contains. Client connects are how
-- short-lived CLI tools (wpctl, pactl, aplay) show up; `removed:` blocks
-- carry no type line and are always the tail of a connect pair or an object
-- teardown the safety poll covers.
local function classify_pwmon_batch(lines)
    local has_signal = false
    local client_added = 0
    local in_added_block = false
    local added_block_is_client = false

    local function close_added_block()
        if not in_added_block then
            return
        end
        if added_block_is_client then
            client_added = client_added + 1
        else
            has_signal = true
        end
        in_added_block = false
    end

    for _, line in ipairs(lines) do
        if line:match("^changed:") then
            close_added_block()
            has_signal = true
        elseif line:match("^added:") then
            close_added_block()
            in_added_block = true
            added_block_is_client = false
        elseif line:match("^removed:") then
            close_added_block()
        elseif in_added_block and line:find("PipeWire:Interface:Client", 1, true) then
            added_block_is_client = true
        end
    end
    close_added_block()

    return has_signal, client_added
end

function on_stream_batch(self, program, lines)
    if program ~= "pw-mon" then
        return
    end

    local has_signal, client_added = classify_pwmon_batch(lines)

    -- Swallow Client connects caused by our own wpctl runs so a refresh
    -- cannot re-trigger itself through the monitor stream. Connects beyond
    -- our outstanding count are external clients (e.g. the compositor's
    -- wpctl for a hardware volume key) and warrant a refresh.
    local external_client = false
    if client_added > 0 then
        if pending_self_noise >= client_added then
            pending_self_noise = pending_self_noise - client_added
        else
            pending_self_noise = 0
            external_client = true
        end
    end

    if not has_signal and not external_client then
        return
    end

    -- `maybe_emit_volume_changed` dedupes identical (percent, muted)
    -- snapshots so unrelated PipeWire activity does not generate frontend
    -- traffic.
    refresh_state()
    maybe_emit_volume_changed(self)
end

function on_command_volume_up(self)
    debug_info("received volume_up()")
    local volume = wpctl_with_sink_fallback({ "set-volume", "@DEFAULT_AUDIO_SINK@", "5%+" })
    local mute = wpctl_with_sink_fallback({ "set-mute", "@DEFAULT_AUDIO_SINK@", "0" })
    refresh_state()
    maybe_emit_volume_changed(self)
    return { ok = volume.success and mute.success, error = volume.stderr ~= "" and volume.stderr or mute.stderr }
end

function on_command_volume_down(self)
    debug_info("received volume_down()")
    local result = wpctl_with_sink_fallback({ "set-volume", "@DEFAULT_AUDIO_SINK@", "5%-" })
    refresh_state()
    maybe_emit_volume_changed(self)
    return { ok = result.success, error = result.stderr }
end

function on_command_set_volume(self)
    local payload = mesh.service.payload()
    local percent = command_percent_from_payload(payload)
    debug_info("received set_volume payload volume=" .. tostring(payload.volume) .. " percent=" .. tostring(percent))
    local result = wpctl_with_sink_fallback({ "set-volume", "@DEFAULT_AUDIO_SINK@", tostring(percent) .. "%" })
    if not result.success then
        refresh_state()
        debug_warn("set_volume failed: " .. (result.stderr or "unknown error"))
        return { ok = false, error = result.stderr }
    end
    refresh_state()
    maybe_emit_volume_changed(self)
    return { ok = true }
end

function on_command_toggle_mute(self)
    debug_info("received toggle_mute()")
    local result = wpctl_with_sink_fallback({ "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle" })
    refresh_state()
    maybe_emit_volume_changed(self)
    return { ok = result.success, error = result.stderr }
end

function on_command_set_muted(self)
    local payload = mesh.service.payload()
    local muted = payload.muted == true
    debug_info("received set_muted(" .. tostring(muted) .. ")")
    local result = wpctl_with_sink_fallback({ "set-mute", "@DEFAULT_AUDIO_SINK@", muted and "1" or "0" })
    refresh_state()
    maybe_emit_volume_changed(self)
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
