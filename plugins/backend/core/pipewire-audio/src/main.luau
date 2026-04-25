-- PipeWire audio backend.
-- Polls volume via wpctl and responds to audio service commands.

mesh.service.set_poll_interval(500)

function on_poll()
    mesh.exec("wpctl get-volume @DEFAULT_AUDIO_SINK@")
    if __exec_success then
        mesh.parse_wpctl_volume()
        mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pipewire-audio" })
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_volume_up()
    mesh.exec("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+")
    mesh.exec("wpctl set-mute @DEFAULT_AUDIO_SINK@ 0")
    mesh.exec("wpctl get-volume @DEFAULT_AUDIO_SINK@")
    mesh.parse_wpctl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pipewire-audio" })
end

function on_command_volume_down()
    mesh.exec("wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-")
    mesh.exec("wpctl get-volume @DEFAULT_AUDIO_SINK@")
    mesh.parse_wpctl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pipewire-audio" })
end

function on_command_toggle_mute()
    mesh.exec("wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle")
    mesh.exec("wpctl get-volume @DEFAULT_AUDIO_SINK@")
    mesh.parse_wpctl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pipewire-audio" })
end

function on_command_set_volume()
    mesh.exec("wpctl set-volume @DEFAULT_AUDIO_SINK@ {percent}%")
    mesh.exec("wpctl get-volume @DEFAULT_AUDIO_SINK@")
    mesh.parse_wpctl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pipewire-audio" })
end

function on_command_play_sound()
    mesh.exec("aplay {path}")
end
