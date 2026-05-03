-- PipeWire audio backend.
-- Polls volume via wpctl and responds to audio service commands.

local PLUGIN_ID = "@mesh/pipewire-audio"

function init()
    mesh.service.set_poll_interval(500)
end

local function resolve_sink_id()
    local result = mesh.exec_shell([=[wpctl status | awk '
BEGIN {
    in_sinks = 0
    first = ""
}
/^[[:space:]]*[|`\\-]*[[:space:]]*├─ Sinks:/ {
    in_sinks = 1
    next
}
/^[[:space:]]*[|`\\-]*[[:space:]]*├─ Sources:/ {
    in_sinks = 0
}
in_sinks {
    if (match($0, /\*[[:space:]]*([0-9]+)\./)) {
        print substr($0, RSTART, RLENGTH)
        exit
    }
    if (first == "" && match($0, /([0-9]+)\./)) {
        first = substr($0, RSTART, RLENGTH)
    }
}
END {
    if (first != "") print first
}
']=])
    if not result.success then
        return nil
    end

    return (result.stdout or ""):match("(%d+)")
end

local function read_state()
    local sink_id = resolve_sink_id()
    if sink_id == nil then
        return nil
    end

    local result = mesh.exec_shell(("wpctl get-volume %s"):format(sink_id))
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

function on_poll()
    emit_state()
end

function on_command_volume_up()
    local sink_id = resolve_sink_id()
    if sink_id then
        mesh.exec_shell(("wpctl set-volume %s 5%%+ && wpctl set-mute %s 0"):format(sink_id, sink_id))
    end
    emit_state()
end

function on_command_volume_down()
    local sink_id = resolve_sink_id()
    if sink_id then
        mesh.exec_shell(("wpctl set-volume %s 5%%-"):format(sink_id))
    end
    emit_state()
end

function on_command_set_volume()
    local payload = mesh.service.payload()
    local percent = command_percent_from_payload(payload)
    local sink_id = resolve_sink_id()
    if sink_id then
        mesh.exec_shell(("wpctl set-volume %s %s%%"):format(sink_id, percent))
    end
    emit_state()
end

function on_command_toggle_mute()
    local sink_id = resolve_sink_id()
    if sink_id then
        mesh.exec_shell(("wpctl set-mute %s toggle"):format(sink_id))
    end
    emit_state()
end

function on_command_set_muted()
    local payload = mesh.service.payload()
    local muted = payload.muted == true
    local sink_id = resolve_sink_id()
    if sink_id then
        mesh.exec_shell(("wpctl set-mute %s %s"):format(sink_id, muted and "1" or "0"))
    end
    emit_state()
end

function on_command_play_sound()
    local payload = mesh.service.payload()
    local path = payload.path
    if type(path) ~= "string" or path == "" then
        return
    end
    mesh.exec("aplay", { path })
end
