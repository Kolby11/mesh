-- MPRIS media provider.
--
-- Follow metadata for prompt track changes; periodic samples also cover status
-- and external seeks on players that omit them from metadata notifications.
-- Art downloads run separately so they cannot delay playback publications.

local FORMAT = "{{status}}\t{{playerName}}\t{{mpris:length}}\t{{position}}\t{{mpris:artUrl}}\t{{xesam:artist}}\t{{xesam:album}}\t{{xesam:title}}"
local POSITION_POLL_MS = 2000

local EMPTY = {
    available = false, state = "stopped", player = "", title = "", artist = "",
    album = "", art_path = "", position = 0, length = 0, updated_at = 0,
}

state = table.clone(EMPTY)

-- Repeated metadata updates for the current track reuse its downloaded art.
local art_url, art_path = "", ""
local requested_art = ""
local art_task

local function now()
    return os.time()
end

local function micros(value)
    local number = tonumber(value)
    if number == nil or number < 0 then
        return 0
    end
    return number / 1000000
end

local function url_decode(text)
    return (text:gsub("%%(%x%x)", function(hex)
        return string.char(tonumber(hex, 16))
    end))
end

-- Returns a local path, downloading once while this art URL remains current.
local function download_art(url)
    if art_url == url then return art_path end
    if not mesh.service.has_capability("net.http") then
        return ""
    end
    local result = mesh.http_request({ url = url, timeout_ms = 10000, cache = true })
    if not result.success then
        mesh.log.debug("[mpris-media] cover art download failed: " .. url)
        return ""
    end
    art_url, art_path = url, result.path
    return art_path
end

local function parse(line)
    local fields = string.split(line, "\t")
    if #fields < 8 then
        return nil
    end
    local status = string.lower(fields[1] or "")
    if status ~= "playing" and status ~= "paused" then
        status = "stopped"
    end
    -- A title may itself contain a tab; keep everything after the 7th field.
    local title = table.concat(fields, "\t", 8)
    return {
        available = true,
        state = status,
        player = fields[2] or "",
        length = micros(fields[3]),
        position = micros(fields[4]),
        art_path = "",
        artist = fields[6] or "",
        album = fields[7] or "",
        title = title,
        updated_at = now(),
    }, fields[5] or ""
end

local function commit(self, next_state, url)
    local track_changed = next_state.title ~= state.title or next_state.artist ~= state.artist
    url = url or ""
    local file = url:match("^file://(.+)$")
    next_state.art_path = file and url_decode(file) or (url == art_url and art_path or "")
    state = next_state
    if url ~= requested_art then
        requested_art = url
        if art_task then task.cancel(art_task) end
        art_task = nil
        if url:match("^https?://") then
            art_task = task.delay(0, function()
                local path = download_art(url)
                if requested_art == url then
                    local current = table.clone(state)
                    current.art_path = path
                    state = current
                    if path == "" then requested_art = "" end
                end
            end)
        end
    end
    if track_changed and state.available and state.title ~= "" then
        self.TrackChanged:fire({ title = state.title, artist = state.artist })
    end
end

local function player_args(args)
    if state.player == "" then return args end
    local selected = { "--player", state.player }
    for _, arg in args do selected[#selected + 1] = arg end
    return selected
end

local function sample(self)
    local args = { "metadata", "--format", FORMAT }
    local result = mesh.exec("playerctl", player_args(args))
    if not result.success and state.player ~= "" then
        result = mesh.exec("playerctl", args)
    end
    local line = result.success and (result.stdout or ""):gsub("\n+$", "") or ""
    local parsed, url = parse(line)
    commit(self, parsed or table.clone(EMPTY), url)
end

function start(self)
    mesh.service.set_poll_interval(POSITION_POLL_MS)
    if not mesh.exec_stream("playerctl", { "--follow", "metadata", "--format", FORMAT }) then
        mesh.log.warn("[mpris-media] metadata stream unavailable; polling")
    end
    if not mesh.exec_stream("playerctl", { "--follow", "status" }) then
        mesh.log.warn("[mpris-media] status stream unavailable; polling")
    end
    sample(self)
end

function on_stream_batch(self, program, lines)
    if program ~= "playerctl" or #lines == 0 then return end
    local line = lines[#lines]
    if line == "Playing" or line == "Paused" or line == "Stopped" then
        sample(self)
    elseif line == "" then
        commit(self, table.clone(EMPTY))
    else
        local parsed, url = parse(line)
        if parsed then commit(self, parsed, url) end
    end
end

function on_stream_closed(self, program, _success)
    if program ~= "playerctl" then
        return
    end
    sample(self)
end

function on_poll(self)
    sample(self)
end

local function control(self, args)
    local result = mesh.exec("playerctl", player_args(args))
    sample(self)
    return { ok = result.success, error = result.stderr }
end

function on_command_play_pause(self)
    return control(self, { "play-pause" })
end

function on_command_next(self)
    return control(self, { "next" })
end

function on_command_previous(self)
    return control(self, { "previous" })
end

function on_command_seek(self)
    local position = tonumber(mesh.service.payload().position)
    if position == nil or position < 0 then
        return { ok = false, error = "invalid position" }
    end
    return control(self, { "position", string.format("%.3f", position) })
end
