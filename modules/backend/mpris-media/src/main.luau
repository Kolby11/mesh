-- MPRIS media provider.
--
-- One `playerctl --follow metadata` stream reports every track and status
-- change; consumers extrapolate the position from `updated_at` while playing,
-- so the provider never polls per second. A slow safety-net poll resamples the
-- position to correct drift after seeks made by other clients.

local FORMAT = "{{status}}\t{{playerName}}\t{{mpris:length}}\t{{position}}\t{{mpris:artUrl}}\t{{xesam:artist}}\t{{xesam:album}}\t{{xesam:title}}"
local FALLBACK_POLL_MS = 2000
local SAFETY_POLL_MS = 15000

local EMPTY = {
    available = false, state = "stopped", player = "", title = "", artist = "",
    album = "", art_path = "", position = 0, length = 0, updated_at = 0,
}

state = table.clone(EMPTY)

-- Repeated metadata updates for the current track reuse its downloaded art.
local art_url, art_path = "", ""

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
local function local_art(url)
    if url == "" then
        return ""
    end
    local file = url:match("^file://(.+)$")
    if file then
        return url_decode(file)
    end
    if not url:match("^https?://") then
        return ""
    end
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
        art_path = local_art(fields[5] or ""),
        artist = fields[6] or "",
        album = fields[7] or "",
        title = title,
        updated_at = now(),
    }
end

local function commit(self, next_state)
    local track_changed = next_state.title ~= state.title or next_state.artist ~= state.artist
    state = next_state
    if track_changed and state.available and state.title ~= "" then
        self.TrackChanged:fire({ title = state.title, artist = state.artist })
    end
end

local function sample(self)
    local result = mesh.exec("playerctl", { "metadata", "--format", FORMAT })
    local line = result.success and (result.stdout or ""):gsub("\n+$", "") or ""
    commit(self, line ~= "" and parse(line) or table.clone(EMPTY))
end

local stream_active = false

function start(self)
    mesh.service.set_poll_interval(FALLBACK_POLL_MS)
    local args = { "--follow", "metadata", "--format", FORMAT }
    if mesh.exec_stream("playerctl", args) then
        stream_active = true
        mesh.service.set_poll_interval(SAFETY_POLL_MS)
    else
        mesh.log.warn("[mpris-media] playerctl stream unavailable; polling")
    end
    sample(self)
end

function on_stream_line(self, program, line)
    if program ~= "playerctl" then
        return
    end
    -- `--follow` prints an empty line when the last player disappears.
    if line == "" then
        commit(self, table.clone(EMPTY))
        return
    end
    local parsed = parse(line)
    if parsed then
        commit(self, parsed)
    end
end

function on_stream_closed(self, program, _success)
    if program ~= "playerctl" then
        return
    end
    stream_active = false
    mesh.service.set_poll_interval(FALLBACK_POLL_MS)
end

function on_poll(self)
    sample(self)
end

local function control(self, args)
    local result = mesh.exec("playerctl", args)
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
