-- PulseAudio audio backend.
-- Polls volume via pactl and responds to audio service commands.

local PLUGIN_ID = "@mesh/pulseaudio-audio"

function init()
    mesh.service.set_poll_interval(500)
end

local function read_state()
    local result = mesh.exec_shell("{ pactl get-sink-volume @DEFAULT_SINK@; pactl get-sink-mute @DEFAULT_SINK@; }")
    if not result.success then
        return nil
    end

    local raw = result.stdout or ""
    local percent = tonumber(raw:match("(%d+)%%")) or 0
    local muted = raw:match("Mute:%s+yes") ~= nil
    return {
        available = true,
        percent = percent,
        muted = muted,
        source_plugin = PLUGIN_ID,
    }
end

local function emit_state()
    local state = read_state()
    if state then
        mesh.service.emit(state)
    else
        mesh.service.emit_unavailable()
    end
end

function on_poll()
    emit_state()
end

function on_command_volume_up()
    mesh.exec_shell("pactl set-sink-volume @DEFAULT_SINK@ +5% && pactl set-sink-mute @DEFAULT_SINK@ 0")
    emit_state()
end

function on_command_volume_down()
    mesh.exec_shell("pactl set-sink-volume @DEFAULT_SINK@ -5%")
    emit_state()
end

function on_command_set_volume()
    local payload = mesh.service.payload()
    local percent = payload.percent or 0
    mesh.exec_shell(("pactl set-sink-volume @DEFAULT_SINK@ %s%%"):format(percent))
    emit_state()
end

function on_command_toggle_mute()
    mesh.exec_shell("pactl set-sink-mute @DEFAULT_SINK@ toggle")
    emit_state()
end

function on_command_play_sound()
    local payload = mesh.service.payload()
    mesh.exec_shell("aplay " .. (payload.path or ""))
end
