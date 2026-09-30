-- brightnessctl-backed display brightness provider.
--
-- The kernel notifies watchers of the backlight's `actual_brightness`
-- attribute on every change, so with `fs.watch` the provider reads the level
-- only when it moves and keeps a slow safety poll. Without the capability it
-- polls brightnessctl.

state = {
    available = false,
    level = 0,
}

local last_level = nil
local last_available = nil
local watch_program = nil

local FALLBACK_POLL_MS = 2000
local SAFETY_POLL_MS = 60000

local function log_info(message)
    mesh.log.info("[backlight-brightness] " .. message)
end

local function log_warn(message)
    mesh.log.warn("[backlight-brightness] " .. message)
end

local function clamp_percent(value, fallback)
    local number = tonumber(value)
    if number == nil then
        return fallback
    end
    return math.max(0, math.min(100, number))
end

-- Returns the level percent and, for a backlight-class device, its name.
local function read_level()
    local result = mesh.exec("brightnessctl", { "-m" })
    if not result.success then
        return nil, nil
    end

    -- brightnessctl -m: device,class,current,percent%,maximum
    local stdout = result.stdout or ""
    local device, class = stdout:match("^([^,\n]+),([^,\n]+),")
    local percent = stdout:match(",([%d%.]+)%%,")
    return clamp_percent(percent, nil), class == "backlight" and device or nil
end

local function publish_level(self, level)
    state = {
        available = level ~= nil,
        level = level or 0,
    }

    if last_available ~= state.available then
        last_available = state.available
        if state.available then
            log_info("backlight became available")
        else
            log_warn("backlight became unavailable")
        end
    end

    if level ~= nil and level ~= last_level then
        last_level = level
        log_info(string.format("level %.1f%%", level))
        if self and self.BrightnessChanged then
            self.BrightnessChanged:fire({ level = level })
        end
    end
end

local function refresh_state(self)
    local level, device = read_level()
    publish_level(self, level)
    return device
end

local function command_amount(default_amount)
    local payload = mesh.service.payload()
    return clamp_percent(payload.amount, default_amount)
end

local function apply(self, argument, next_level)
    local result = mesh.exec("brightnessctl", { "set", argument })
    if result.success then
        -- Publish the requested value optimistically; the attribute watch (or
        -- the poll without one) corrects rounding or clamping.
        publish_level(self, next_level)
    else
        refresh_state(self)
        log_warn("brightnessctl set failed: " .. (result.stderr or "unknown error"))
    end
    return { ok = result.success, error = result.stderr }
end

function start(self)
    mesh.service.set_poll_interval(FALLBACK_POLL_MS)
    local device = refresh_state(self)
    if device and mesh.service.has_capability("fs.watch") then
        local started = mesh.watch_path("/sys/class/backlight/" .. device .. "/actual_brightness")
        if started then
            watch_program = started.program
            mesh.service.set_poll_interval(SAFETY_POLL_MS)
            log_info("watching backlight " .. device)
        end
    end
end

function on_poll(self)
    refresh_state(self)
end

function on_stream_batch(self, program, _lines)
    if program == watch_program then
        refresh_state(self)
    end
end

function on_stream_closed(self, program, _success)
    if program == watch_program then
        log_warn("backlight watch ended; falling back to polling")
        watch_program = nil
        mesh.service.set_poll_interval(FALLBACK_POLL_MS)
    end
end

function on_command_set(self)
    local payload = mesh.service.payload()
    local level = clamp_percent(payload.level, state.level)
    return apply(self, string.format("%.1f%%", level), level)
end

function on_command_increase(self)
    local amount = command_amount(5)
    local next_level = clamp_percent(state.level + amount, state.level)
    return apply(self, string.format("%.1f%%+", amount), next_level)
end

function on_command_decrease(self)
    local amount = command_amount(5)
    local next_level = clamp_percent(state.level - amount, state.level)
    return apply(self, string.format("%.1f%%-", amount), next_level)
end
