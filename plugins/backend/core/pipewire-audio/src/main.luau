-- PipeWire audio backend.
-- Polls volume via wpctl and responds to audio service commands.

mesh.service.set_poll_interval(500)

function on_poll()
    mesh.exec_shell('wpctl get-volume @DEFAULT_AUDIO_SINK@ | awk "BEGIN { vol = 0; muted = \\"false\\" } /\\[MUTED\\]/ { muted = \\"true\\" } { for (i = 1; i <= NF; i++) if (\\$i ~ /^[0-9]*\\.[0-9]+$/) vol = int((\\$i * 100) + 0.5) } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pipewire-audio\\\\\\"}\\\\n\\", vol, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_volume_up()
    mesh.exec_shell('wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%+ && wpctl set-mute @DEFAULT_AUDIO_SINK@ 0 && wpctl get-volume @DEFAULT_AUDIO_SINK@ | awk "BEGIN { vol = 0; muted = \\"false\\" } /\\[MUTED\\]/ { muted = \\"true\\" } { for (i = 1; i <= NF; i++) if (\\$i ~ /^[0-9]*\\.[0-9]+$/) vol = int((\\$i * 100) + 0.5) } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pipewire-audio\\\\\\"}\\\\n\\", vol, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_volume_down()
    mesh.exec_shell('wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- && wpctl get-volume @DEFAULT_AUDIO_SINK@ | awk "BEGIN { vol = 0; muted = \\"false\\" } /\\[MUTED\\]/ { muted = \\"true\\" } { for (i = 1; i <= NF; i++) if (\\$i ~ /^[0-9]*\\.[0-9]+$/) vol = int((\\$i * 100) + 0.5) } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pipewire-audio\\\\\\"}\\\\n\\", vol, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_toggle_mute()
    mesh.exec_shell('wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle && wpctl get-volume @DEFAULT_AUDIO_SINK@ | awk "BEGIN { vol = 0; muted = \\"false\\" } /\\[MUTED\\]/ { muted = \\"true\\" } { for (i = 1; i <= NF; i++) if (\\$i ~ /^[0-9]*\\.[0-9]+$/) vol = int((\\$i * 100) + 0.5) } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pipewire-audio\\\\\\"}\\\\n\\", vol, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_set_volume()
    mesh.exec_shell('wpctl set-volume @DEFAULT_AUDIO_SINK@ {percent}% && wpctl get-volume @DEFAULT_AUDIO_SINK@ | awk "BEGIN { vol = 0; muted = \\"false\\" } /\\[MUTED\\]/ { muted = \\"true\\" } { for (i = 1; i <= NF; i++) if (\\$i ~ /^[0-9]*\\.[0-9]+$/) vol = int((\\$i * 100) + 0.5) } END { printf(\\"{\\\\\\"available\\\\\\":true,\\\\\\"percent\\\\\\":%d,\\\\\\"muted\\\\\\":%s,\\\\\\"source_plugin\\\\\\":\\\\\\"@mesh/pipewire-audio\\\\\\"}\\\\n\\", vol, muted) }"')
    if __exec_success then
        mesh.service.emit_json()
    else
        mesh.service.emit_unavailable()
    end
end

function on_command_play_sound()
    mesh.exec("aplay {path}")
end
