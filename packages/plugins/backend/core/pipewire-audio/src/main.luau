-- PipeWire audio backend.
-- Polls volume via wpctl and responds to audio service commands.

local PLUGIN_ID = "@mesh/pipewire-audio"

function init()
    mesh.service.set_poll_interval(500)
end

local function read_state()
    local result = mesh.exec_shell("wpctl get-volume @DEFAULT_AUDIO_SINK@")
    if not result.success then
        return nil
    end

    local raw = result.stdout or ""
    local level = tonumber(raw:match("(%d+%.%d+)")) or 0
    return {
        available = true,
        percent = math.floor((level * 100) + 0.5),
        muted = raw:match("%[MUTED%]") ~= nil,
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
    mesh.exec_shell("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+ && wpctl set-mute @DEFAULT_AUDIO_SINK@ 0")
    emit_state()
end

function on_command_volume_down()
    mesh.exec_shell("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-")
    emit_state()
end

function on_command_set_volume()
    local payload = mesh.service.payload()
    local percent = payload.percent or 0
    mesh.exec_shell(("wpctl set-volume @DEFAULT_AUDIO_SINK@ %s%%"):format(percent))
    emit_state()
end

function on_command_toggle_mute()
    mesh.exec_shell("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle")
    emit_state()
end

function on_command_play_sound()
    local payload = mesh.service.payload()
    mesh.exec_shell("aplay " .. (payload.path or ""))
end
