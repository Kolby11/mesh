-- Idle timers through a hypridle instance that MESH owns.
--
-- It only runs while "managed" is on. A user who already runs their own
-- hypridle must turn one of the two off, or both react to idleness.

state = {
    available = true,
    managed = false,
    running = false,
    dim_after_min = 0,
    blank_after_min = 0,
    lock_after_min = 0,
    suspend_after_min = 0,
    lock_on_suspend = true,
}

local RESTART_MS = 5000
local WRITE_CONF = 'printf \'%s\' "$1" > "${XDG_RUNTIME_DIR:-/tmp}/mesh-hypridle.conf"'
local RUN = 'exec hypridle -c "${XDG_RUNTIME_DIR:-/tmp}/mesh-hypridle.conf"'
local source = nil

local function log_warn(message)
    mesh.log.warn("[hypridle-idle] " .. message)
end

local function clamp_minutes(value)
    return math.max(0, math.min(600, math.floor(tonumber(value) or 0)))
end

local function build_config()
    local parts = {
        "general {\n    lock_cmd = pidof hyprlock || hyprlock\n"
            .. (state.lock_on_suspend and "    before_sleep_cmd = loginctl lock-session\n" or "")
            .. "}\n",
    }
    local function listener(minutes, on_timeout, on_resume)
        if minutes <= 0 then return end
        local body = string.format("listener {\n    timeout = %d\n    on-timeout = %s\n", minutes * 60, on_timeout)
        if on_resume then body = body .. "    on-resume = " .. on_resume .. "\n" end
        parts[#parts + 1] = body .. "}\n"
    end
    listener(state.dim_after_min, "brightnessctl -s set 20%", "brightnessctl -r")
    listener(state.lock_after_min, "loginctl lock-session")
    listener(state.blank_after_min, "hyprctl dispatch 'hl.dsp.dpms({ action = \"disable\" })'", "hyprctl dispatch 'hl.dsp.dpms({ action = \"enable\" })'")
    listener(state.suspend_after_min, "systemctl suspend")
    return table.concat(parts, "\n")
end

local function stop_daemon()
    if source ~= nil then
        mesh.close_stream(source)
        source = nil
    end
    state.running = false
end

local function start_daemon()
    stop_daemon()
    if not state.managed then return end
    local wrote = mesh.exec("sh", { "-c", WRITE_CONF, "sh", build_config() })
    if not wrote.success then
        log_warn("could not write the hypridle config: " .. (wrote.stderr or "unknown error"))
        return
    end
    source = mesh.exec_stream("sh", { "-c", RUN })
    state.running = source ~= nil
end

local function persist(self)
    local s = self.storage
    s.managed = state.managed
    s.dim_after_min = state.dim_after_min
    s.blank_after_min = state.blank_after_min
    s.lock_after_min = state.lock_after_min
    s.suspend_after_min = state.suspend_after_min
    s.lock_on_suspend = state.lock_on_suspend
end

function start(self)
    local saved = self.storage
    state.managed = saved.managed == true
    state.dim_after_min = clamp_minutes(saved.dim_after_min)
    state.blank_after_min = clamp_minutes(saved.blank_after_min)
    state.lock_after_min = clamp_minutes(saved.lock_after_min)
    state.suspend_after_min = clamp_minutes(saved.suspend_after_min)
    if saved.lock_on_suspend ~= nil then state.lock_on_suspend = saved.lock_on_suspend == true end
    mesh.service.set_poll_interval(state.managed and RESTART_MS or 0)
    start_daemon()
end

function on_poll(self)
    if state.managed and source == nil then start_daemon() end
end

function on_stream_closed(self, program, _success)
    if program == "sh" then
        source = nil
        state.running = false
        mesh.service.set_poll_interval(state.managed and RESTART_MS or 0)
    end
end

local function changed(self)
    persist(self)
    mesh.service.set_poll_interval(state.managed and RESTART_MS or 0)
    start_daemon()
    return { ok = true }
end

function on_command_set_managed(self)
    state.managed = mesh.service.payload().enabled == true
    return changed(self)
end

function on_command_set_timeout(self)
    local payload = mesh.service.payload()
    local key = ({ dim = "dim_after_min", blank = "blank_after_min", lock = "lock_after_min", suspend = "suspend_after_min" })[tostring(payload.kind)]
    if not key then return { ok = false, error = "unknown timer" } end
    state[key] = clamp_minutes(payload.minutes)
    return changed(self)
end

function on_command_set_lock_on_suspend(self)
    state.lock_on_suspend = mesh.service.payload().enabled == true
    return changed(self)
end
