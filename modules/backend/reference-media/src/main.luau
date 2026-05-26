local playlist = {}
local current_index = 1
local playback_state = "paused"

state = {
    available = true,
    title = "Reference Track",
    artist = "Reference Artist",
    album = "Reference Album",
    state = playback_state,
}

local function build_playlist(config)
    return {
        {
            title = config.seed_title or "Reference Track",
            artist = config.seed_artist or "Reference Artist",
            album = config.seed_album or "Reference Album",
        },
        {
            title = "Second Track",
            artist = "Reference Artist",
            album = "Reference Album",
        },
        {
            title = "Third Track",
            artist = "Reference Artist",
            album = "Reference Album",
        },
    }
end

local function sync_state()
    local track = playlist[current_index]
    state = {
        available = true,
        title = track.title,
        artist = track.artist,
        album = track.album,
        state = playback_state,
    }
end

function start(self)
    mesh.service.set_poll_interval(5000)
    playlist = build_playlist(mesh.config())
    current_index = 1
    playback_state = "paused"
    sync_state()
end

function on_poll(self)
    sync_state()
end

function on_command_play(self)
    playback_state = "playing"
    sync_state()
    return { ok = true }
end

function on_command_pause(self)
    if playback_state ~= "playing" then
        return { ok = false, error = "not currently playing" }
    end

    playback_state = "paused"
    sync_state()
    return { ok = true }
end

function on_command_next(self)
    current_index = (current_index % #playlist) + 1
    sync_state()
    return { ok = true }
end

function on_command_previous(self)
    current_index = ((current_index - 2) % #playlist) + 1
    sync_state()
    return { ok = true }
end
