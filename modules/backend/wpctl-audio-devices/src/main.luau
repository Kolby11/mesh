-- Audio devices and per-application streams through wpctl.

state = { available = false, sinks = {}, sources = {}, streams = {} }

local POLL_MS = 3000

local function log_warn(message)
    mesh.log.warn("[wpctl-audio-devices] " .. message)
end

local function parse_node(line)
    local star, id, rest = line:match("^[%s│├└─]*(%*?)%s*(%d+)%.%s+(.+)$")
    if not id then return nil end
    local name, volume, muted = rest:match("^(.-)%s+%[vol: ([%d%.]+)%](.*)$")
    if not name then return nil end
    return {
        id = tonumber(id), name = name, default = star == "*",
        volume = math.floor((tonumber(volume) or 0) * 100 + 0.5), muted = muted:find("MUTED", 1, true) ~= nil,
    }
end

local function stream_volume(id)
    local result = mesh.exec("wpctl", { "get-volume", tostring(id) })
    if not result.success then return 0, false end
    local volume = tonumber((result.stdout or ""):match("Volume:%s*([%d%.]+)")) or 0
    return math.floor(volume * 100 + 0.5), (result.stdout or ""):find("MUTED", 1, true) ~= nil
end

local function refresh()
    local result = mesh.exec("wpctl", { "status" })
    if not result.success then
        state = { available = false, sinks = {}, sources = {}, streams = {} }
        return
    end
    local sinks, sources, streams = {}, {}, {}
    local section, in_audio = nil, false
    for line in (result.stdout or ""):gmatch("[^\n]+") do
        if line:match("^Audio") then
            in_audio = true
        elseif line:match("^Video") or line:match("^Settings") then
            in_audio = false
        elseif in_audio then
            if line:find("Sinks:", 1, true) then section = "sinks"
            elseif line:find("Sources:", 1, true) then section = "sources"
            elseif line:find("Streams:", 1, true) then section = "streams"
            elseif line:find("Devices:", 1, true) or line:find("Filters:", 1, true) then section = nil
            elseif section == "sinks" or section == "sources" then
                local node = parse_node(line)
                if node then
                    if section == "sinks" then sinks[#sinks + 1] = node else sources[#sources + 1] = node end
                end
            elseif section == "streams" and not line:find("[<>]") then
                local id, name = line:match("^%s+(%d+)%.%s+(.-)%s*$")
                if id and name ~= "" then
                    local volume, muted = stream_volume(id)
                    streams[#streams + 1] = { id = tonumber(id), name = name, default = false, volume = volume, muted = muted }
                end
            end
        end
    end
    state = { available = true, sinks = sinks, sources = sources, streams = streams }
end

function start(self)
    mesh.service.set_poll_interval(POLL_MS)
    refresh()
end

function on_poll(self)
    refresh()
end

local function run(args)
    local result = mesh.exec("wpctl", args)
    if not result.success then log_warn("wpctl " .. tostring(args[1]) .. " failed: " .. (result.stderr or "unknown error")) end
    refresh()
    return { ok = result.success, error = result.stderr }
end

local function valid_id(id)
    return math.floor(tonumber(id) or -1) >= 0
end

function on_command_set_default(self)
    local id = mesh.service.payload().id
    if not valid_id(id) then return { ok = false, error = "invalid node" } end
    return run({ "set-default", tostring(math.floor(id)) })
end

function on_command_set_volume(self)
    local payload = mesh.service.payload()
    if not valid_id(payload.id) then return { ok = false, error = "invalid node" } end
    local percent = math.max(0, math.min(150, math.floor(tonumber(payload.percent) or 0)))
    return run({ "set-volume", tostring(math.floor(payload.id)), tostring(percent) .. "%" })
end

function on_command_set_muted(self)
    local payload = mesh.service.payload()
    if not valid_id(payload.id) then return { ok = false, error = "invalid node" } end
    return run({ "set-mute", tostring(math.floor(payload.id)), payload.muted == true and "1" or "0" })
end
