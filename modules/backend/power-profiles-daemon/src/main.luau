-- Platform power profile through power-profiles-daemon.

state = { available = false, profile = "balanced", profiles = {} }

local POLL_MS = 5000
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
    mesh.service.set_poll_interval(POLL_MS)
    refresh()
end

function on_poll(self)
    refresh()
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
