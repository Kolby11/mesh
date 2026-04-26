-- PulseAudio audio backend.
-- Polls volume via pactl and responds to audio service commands.

mesh.service.set_poll_interval(500)

function on_poll()
    mesh.exec_shell('{ pactl get-sink-volume @DEFAULT_SINK@; pactl get-sink-mute @DEFAULT_SINK@; } | awk "BEGIN { percent = 0; muted = \\"false\\" } /[0-9]+%/ && percent == 0 { if (match(\\$0, /[0-9]+%/)) percent = substr(\\$0, RSTART, RLENGTH - 1) } /^Mute:/ { if (\\$2 == \\"yes\\") muted = \\"true\\" } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pulseaudio-audio\\\\\\"}\\\\n\\", percent, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_volume_up()
    mesh.exec_shell('{ pactl set-sink-volume @DEFAULT_SINK@ +5% && pactl set-sink-mute @DEFAULT_SINK@ 0 && pactl get-sink-volume @DEFAULT_SINK@ && pactl get-sink-mute @DEFAULT_SINK@; } | awk "BEGIN { percent = 0; muted = \\"false\\" } /[0-9]+%/ && percent == 0 { if (match(\\$0, /[0-9]+%/)) percent = substr(\\$0, RSTART, RLENGTH - 1) } /^Mute:/ { if (\\$2 == \\"yes\\") muted = \\"true\\" } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pulseaudio-audio\\\\\\"}\\\\n\\", percent, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_volume_down()
    mesh.exec_shell('{ pactl set-sink-volume @DEFAULT_SINK@ -5% && pactl get-sink-volume @DEFAULT_SINK@ && pactl get-sink-mute @DEFAULT_SINK@; } | awk "BEGIN { percent = 0; muted = \\"false\\" } /[0-9]+%/ && percent == 0 { if (match(\\$0, /[0-9]+%/)) percent = substr(\\$0, RSTART, RLENGTH - 1) } /^Mute:/ { if (\\$2 == \\"yes\\") muted = \\"true\\" } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pulseaudio-audio\\\\\\"}\\\\n\\", percent, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_toggle_mute()
    mesh.exec_shell('{ pactl set-sink-mute @DEFAULT_SINK@ toggle && pactl get-sink-volume @DEFAULT_SINK@ && pactl get-sink-mute @DEFAULT_SINK@; } | awk "BEGIN { percent = 0; muted = \\"false\\" } /[0-9]+%/ && percent == 0 { if (match(\\$0, /[0-9]+%/)) percent = substr(\\$0, RSTART, RLENGTH - 1) } /^Mute:/ { if (\\$2 == \\"yes\\") muted = \\"true\\" } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pulseaudio-audio\\\\\\"}\\\\n\\", percent, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_set_volume()
    mesh.exec_shell('{ pactl set-sink-volume @DEFAULT_SINK@ {percent}% && pactl get-sink-volume @DEFAULT_SINK@ && pactl get-sink-mute @DEFAULT_SINK@; } | awk "BEGIN { percent = 0; muted = \\"false\\" } /[0-9]+%/ && percent == 0 { if (match(\\$0, /[0-9]+%/)) percent = substr(\\$0, RSTART, RLENGTH - 1) } /^Mute:/ { if (\\$2 == \\"yes\\") muted = \\"true\\" } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pulseaudio-audio\\\\\\"}\\\\n\\", percent, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_play_sound()
    mesh.exec("aplay {path}")
end
