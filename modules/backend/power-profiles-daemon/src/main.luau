-- Platform power profile through power-profiles-daemon.

state = { available = false, profile = "balanced", profiles = {} }

-- The daemon announces profile changes on the bus; polling is only the
-- fallback when that subscription is unavailable.
local IDLE_POLL_MS = 60000
local UNMONITORED_POLL_MS = 5000
local monitor = nil
local KNOWN = { ["power-saver"] = true, balanced = true, performance = true }

local function log_warn(message)
    mesh.log.warn("[power-profiles-daemon] " .. message)
end

local function refresh()
    local current = mesh.exec("powerprofilesctl", { "get" })
    if not current.success then
        state = { available = false, profile = "balanced", profiles = {} }
        return
    end
    local profiles = {}
    local listed = mesh.exec("powerprofilesctl", { "list" })
    if listed.success then
        for line in (listed.stdout or ""):gmatch("[^\n]+") do
            local name = line:match("^[%s%*]*([%w%-]+):%s*$")
            if name and KNOWN[name] then profiles[#profiles + 1] = name end
        end
    end
    local profile = (current.stdout or ""):match("[%w%-]+") or "balanced"
    state = { available = true, profile = profile, profiles = profiles }
end

function start(self)
    mesh.service.set_poll_interval(IDLE_POLL_MS)
    monitor = mesh.dbus_subscribe("system", { sender = "net.hadess.PowerProfiles", path_namespace = "/" })
    if not monitor then mesh.service.set_poll_interval(UNMONITORED_POLL_MS) end
    refresh()
end

function on_poll(self)
    refresh()
end

function on_stream_batch(self, program, _lines)
    if monitor ~= nil and program == monitor.program then
        refresh()
    end
end

function on_stream_closed(self, program, _success)
    if monitor ~= nil and program == monitor.program then
        monitor = nil
        mesh.service.set_poll_interval(UNMONITORED_POLL_MS)
    end
end

function on_command_set_profile(self)
    local profile = tostring(mesh.service.payload().profile or "")
    if not KNOWN[profile] then
        return { ok = false, error = "unknown profile" }
    end
    local result = mesh.exec("powerprofilesctl", { "set", profile })
    if not result.success then
        log_warn("powerprofilesctl set failed: " .. (result.stderr or "unknown error"))
    end
    refresh()
    return { ok = result.success, error = result.stderr }
end
