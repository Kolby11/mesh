-- Time zone and network time through timedatectl.
--
-- Changing either needs polkit authorisation for the active session; a
-- refusal is reported to the caller and logged.

state = { available = false, timezone = "", ntp = false, ntp_synced = false, timezones = {} }

local POLL_MS = 10000
-- timedated announces time zone and NTP changes on the system bus; the slow
-- poll only keeps the synchronization flag current.
local IDLE_POLL_MS = 60000
local monitor = nil
local zones = {}

local function log_warn(message)
    mesh.log.warn("[timedatectl-time] " .. message)
end

local function refresh()
    local result = mesh.exec("timedatectl", { "show" })
    if not result.success then
        state = { available = false, timezone = "", ntp = false, ntp_synced = false, timezones = zones }
        return
    end
    local values = {}
    for key, value in (result.stdout or ""):gmatch("([%w]+)=([^\n]*)") do values[key] = value end
    state = {
        available = true,
        timezone = values.Timezone or "",
        ntp = values.NTP == "yes",
        ntp_synced = values.NTPSynchronized == "yes",
        timezones = zones,
    }
end

function start(self)
    local listed = mesh.exec("timedatectl", { "list-timezones" })
    if listed.success then
        for line in (listed.stdout or ""):gmatch("[^\n]+") do zones[#zones + 1] = line end
    end
    local started = mesh.dbus_subscribe("system", {
        sender = "org.freedesktop.timedate1",
        path_namespace = "/org/freedesktop/timedate1",
    })
    monitor = started and started.program or nil
    mesh.service.set_poll_interval(monitor and IDLE_POLL_MS or POLL_MS)
    refresh()
end

function on_poll(self)
    refresh()
end

function on_stream_batch(self, program, _lines)
    if monitor ~= nil and program == monitor then refresh() end
end

function on_stream_closed(self, program, _success)
    if monitor ~= nil and program == monitor then
        monitor = nil
        mesh.service.set_poll_interval(POLL_MS)
    end
end

function on_command_set_timezone(self)
    local zone = tostring(mesh.service.payload().timezone or "")
    if not zone:match("^[%w%+%-_/]+$") then
        return { ok = false, error = "invalid time zone" }
    end
    local result = mesh.exec("timedatectl", { "set-timezone", zone })
    if not result.success then log_warn("set-timezone failed: " .. (result.stderr or "unknown error")) end
    refresh()
    return { ok = result.success, error = result.stderr }
end

function on_command_set_ntp(self)
    local enabled = mesh.service.payload().enabled == true
    local result = mesh.exec("timedatectl", { "set-ntp", enabled and "true" or "false" })
    if not result.success then log_warn("set-ntp failed: " .. (result.stderr or "unknown error")) end
    refresh()
    return { ok = result.success, error = result.stderr }
end
