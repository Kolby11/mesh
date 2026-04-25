-- PulseAudio audio backend.
-- Polls volume via pactl and responds to audio service commands.

mesh.service.set_poll_interval(500)

function on_poll()
    mesh.exec("pactl get-sink-volume @DEFAULT_SINK@")
    if __exec_success then
        mesh.parse_pactl_volume()
        mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pulseaudio-audio" })
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_volume_up()
    mesh.exec("pactl set-sink-volume @DEFAULT_SINK@ +5%")
    mesh.exec("pactl set-sink-mute @DEFAULT_SINK@ 0")
    mesh.exec("pactl get-sink-volume @DEFAULT_SINK@")
    mesh.parse_pactl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pulseaudio-audio" })
end

function on_command_volume_down()
    mesh.exec("pactl set-sink-volume @DEFAULT_SINK@ -5%")
    mesh.exec("pactl get-sink-volume @DEFAULT_SINK@")
    mesh.parse_pactl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pulseaudio-audio" })
end

function on_command_toggle_mute()
    mesh.exec("pactl set-sink-mute @DEFAULT_SINK@ toggle")
    mesh.exec("pactl get-sink-volume @DEFAULT_SINK@")
    mesh.parse_pactl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pulseaudio-audio" })
end

function on_command_set_volume()
    mesh.exec("pactl set-sink-volume @DEFAULT_SINK@ {percent}%")
    mesh.exec("pactl get-sink-volume @DEFAULT_SINK@")
    mesh.parse_pactl_volume()
    mesh.service.emit({ available = true, percent = __parsed_volume, muted = __parsed_muted, source_plugin = "@mesh/pulseaudio-audio" })
end

function on_command_play_sound()
    mesh.exec("aplay {path}")
end
