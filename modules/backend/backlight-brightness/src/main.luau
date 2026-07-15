-- brightnessctl-backed display brightness provider.

state = {
    available = false,
    level = 0,
}

local last_level = nil
local last_available = nil

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

local function read_level()
    local result = mesh.exec("brightnessctl", { "-m" })
    if not result.success then
        return nil
    end

    -- brightnessctl -m: device,class,current,percent%,maximum
    local percent = (result.stdout or ""):match(",([%d%.]+)%%,")
    return clamp_percent(percent, nil)
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
    publish_level(self, read_level())
end

local function command_amount(default_amount)
    local payload = mesh.service.payload()
    return clamp_percent(payload.amount, default_amount)
end

local function apply(self, argument)
    local result = mesh.exec("brightnessctl", { "set", argument })
    refresh_state(self)
    if not result.success then
        log_warn("brightnessctl set failed: " .. (result.stderr or "unknown error"))
    end
    return { ok = result.success, error = result.stderr }
end

function start(self)
    mesh.service.set_poll_interval(2000)
    refresh_state(self)
end

function on_poll(self)
    refresh_state(self)
end

function on_command_set(self)
    local payload = mesh.service.payload()
    local level = clamp_percent(payload.level, state.level)
    return apply(self, string.format("%.1f%%", level))
end

function on_command_increase(self)
    local amount = command_amount(5)
    return apply(self, string.format("%.1f%%+", amount))
end

function on_command_decrease(self)
    local amount = command_amount(5)
    return apply(self, string.format("%.1f%%-", amount))
end
