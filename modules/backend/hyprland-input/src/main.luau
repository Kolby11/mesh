-- Pointer, touchpad and cursor options for Hyprland.
--
-- Changes go through `hyprctl eval` and are runtime-only, so the last values
-- are kept in `self.storage` and re-applied at start.

state = {
    available = false,
    pointer_speed = 0,
    natural_scroll = false,
    tap_to_click = true,
    disable_while_typing = true,
    left_handed = false,
    cursor_size = 24,
}

local POLL_MS = 5000
local cursor_size = 24

local function log_warn(message)
    mesh.log.warn("[hyprland-input] " .. message)
end

local function option(name)
    local result = mesh.exec("hyprctl", { "getoption", name, "-j" })
    if not result.success then return nil end
    local ok, value = pcall(mesh.json.decode, result.stdout or "")
    if not ok or type(value) ~= "table" then return nil end
    if value.bool ~= nil then return value.bool end
    if value.float ~= nil then return tonumber(value.float) end
    if value.int ~= nil then return tonumber(value.int) end
    return nil
end

local function eval_ok(code)
    local result = mesh.exec("hyprctl", { "eval", code })
    if not result.success or not (result.stdout or ""):find("ok", 1, true) then
        log_warn("hyprctl eval failed: " .. (result.stderr or result.stdout or "unknown error"))
        return false
    end
    return true
end

local function refresh()
    local sensitivity = option("input:sensitivity")
    if sensitivity == nil then
        state = {
            available = false, pointer_speed = 0, natural_scroll = false, tap_to_click = true,
            disable_while_typing = true, left_handed = false, cursor_size = cursor_size,
        }
        return
    end
    local natural = option("input:natural_scroll")
    local touchpad_natural = option("input:touchpad:natural_scroll")
    state = {
        available = true,
        pointer_speed = math.floor(sensitivity * 10 + (sensitivity >= 0 and 0.5 or -0.5)),
        natural_scroll = natural == true or touchpad_natural == true,
        tap_to_click = option("input:touchpad:tap-to-click") == true,
        disable_while_typing = option("input:touchpad:disable_while_typing") == true,
        left_handed = option("input:left_handed") == true,
        cursor_size = cursor_size,
    }
end

local function apply_speed(value)
    return eval_ok(string.format("hl.config({ input = { sensitivity = %.1f } })", value / 10))
end

local function apply_natural(enabled)
    local flag = enabled and "true" or "false"
    return eval_ok(string.format("hl.config({ input = { natural_scroll = %s, touchpad = { natural_scroll = %s } } })", flag, flag))
end

local function apply_tap(enabled)
    return eval_ok(string.format("hl.config({ input = { touchpad = { tap_to_click = %s } } })", enabled and "true" or "false"))
end

local function apply_dwt(enabled)
    return eval_ok(string.format("hl.config({ input = { touchpad = { disable_while_typing = %s } } })", enabled and "true" or "false"))
end

local function apply_left(enabled)
    return eval_ok(string.format("hl.config({ input = { left_handed = %s } })", enabled and "true" or "false"))
end

local function apply_cursor(size)
    local result = mesh.exec("sh", { "-c", 'hyprctl setcursor "${XCURSOR_THEME:-default}" "$1"', "sh", tostring(size) })
    if not result.success then
        log_warn("hyprctl setcursor failed: " .. (result.stderr or "unknown error"))
    end
    return result.success
end

local function clamp(value, lo, hi)
    return math.max(lo, math.min(hi, value))
end

function start(self)
    mesh.service.set_poll_interval(POLL_MS)
    local saved = self.storage
    if saved.cursor_size ~= nil then cursor_size = clamp(math.floor(tonumber(saved.cursor_size) or 24), 16, 96) end
    if saved.pointer_speed ~= nil then apply_speed(clamp(math.floor(tonumber(saved.pointer_speed) or 0), -10, 10)) end
    if saved.natural_scroll ~= nil then apply_natural(saved.natural_scroll == true) end
    if saved.tap_to_click ~= nil then apply_tap(saved.tap_to_click == true) end
    if saved.disable_while_typing ~= nil then apply_dwt(saved.disable_while_typing == true) end
    if saved.left_handed ~= nil then apply_left(saved.left_handed == true) end
    if saved.cursor_size ~= nil then apply_cursor(cursor_size) end
    refresh()
end

function on_poll(self)
    refresh()
end

local function finish(self, ok, key, value)
    if ok then self.storage[key] = value end
    refresh()
    return { ok = ok }
end

function on_command_set_pointer_speed(self)
    local value = clamp(math.floor(tonumber(mesh.service.payload().value) or 0), -10, 10)
    return finish(self, apply_speed(value), "pointer_speed", value)
end

function on_command_set_natural_scroll(self)
    local enabled = mesh.service.payload().enabled == true
    return finish(self, apply_natural(enabled), "natural_scroll", enabled)
end

function on_command_set_tap_to_click(self)
    local enabled = mesh.service.payload().enabled == true
    return finish(self, apply_tap(enabled), "tap_to_click", enabled)
end

function on_command_set_disable_while_typing(self)
    local enabled = mesh.service.payload().enabled == true
    return finish(self, apply_dwt(enabled), "disable_while_typing", enabled)
end

function on_command_set_left_handed(self)
    local enabled = mesh.service.payload().enabled == true
    return finish(self, apply_left(enabled), "left_handed", enabled)
end

function on_command_set_cursor_size(self)
    local size = clamp(math.floor(tonumber(mesh.service.payload().size) or 24), 16, 96)
    local ok = apply_cursor(size)
    if ok then cursor_size = size end
    return finish(self, ok, "cursor_size", size)
end
