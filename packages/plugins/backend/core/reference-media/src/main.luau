-- Reference media backend.
-- Provides a deterministic, in-memory implementation of the mesh.media interface.
-- All state is driven by config and in-memory mutations — no external binaries required.
-- This plugin demonstrates the full backend MVP contract:
--   mesh.config(), mesh.log, mesh.service.set_poll_interval, state global, on_command_* handlers.

-- Seed a small in-memory playlist from config or fall back to built-in defaults.
local cfg = mesh.config()
local seed_title = (cfg and cfg.seed_title) or "Reference Track"
local seed_artist = (cfg and cfg.seed_artist) or "Reference Artist"
local seed_album = (cfg and cfg.seed_album) or "Reference Album"

local playlist = {
    { title = seed_title,         artist = seed_artist,   album = seed_album },
    { title = "Second Track",     artist = seed_artist,   album = seed_album },
    { title = "Third Track",      artist = "Other Artist", album = "Other Album" },
}

local current_index = 1
local playback_state = "stopped"

-- Top-level state global — snapshot taken by runtime after each handler.
state = {
    available = true,
    title = playlist[current_index].title,
    artist = playlist[current_index].artist,
    album = playlist[current_index].album,
    state = playback_state,
}

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

function init()
    mesh.log.info("reference-media: initializing with " .. tostring(#playlist) .. " track(s)")
    mesh.service.set_poll_interval(5000)
    sync_state()
end

function on_poll()
    -- Reference backend: state is purely in-memory; poll is a no-op but still syncs.
    sync_state()
end

function on_command_play()
    local payload = mesh.service.payload()
    local player_id = payload and payload.player_id or "default"
    mesh.log.info("reference-media: play requested for player=" .. tostring(player_id))
    if playback_state == "playing" then
        return { ok = true }
    end
    playback_state = "playing"
    sync_state()
    return { ok = true }
end

function on_command_pause()
    local payload = mesh.service.payload()
    local player_id = payload and payload.player_id or "default"
    mesh.log.info("reference-media: pause requested for player=" .. tostring(player_id))
    if playback_state ~= "playing" then
        return { ok = false, error = "not currently playing" }
    end
    playback_state = "paused"
    sync_state()
    return { ok = true }
end

function on_command_next()
    local payload = mesh.service.payload()
    local player_id = payload and payload.player_id or "default"
    mesh.log.info("reference-media: next requested for player=" .. tostring(player_id))
    current_index = current_index % #playlist + 1
    sync_state()
    return { ok = true }
end

function on_command_previous()
    local payload = mesh.service.payload()
    local player_id = payload and payload.player_id or "default"
    mesh.log.info("reference-media: previous requested for player=" .. tostring(player_id))
    current_index = (current_index - 2) % #playlist + 1
    sync_state()
    return { ok = true }
end
