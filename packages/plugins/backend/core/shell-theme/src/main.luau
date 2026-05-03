-- Shell theme service backend.
-- Tracks available themes and the currently active theme.
-- The shell calls on_command_set_current whenever the active theme changes
-- so this backend stays in sync without polling the filesystem.

local DARK_THEME  = "mesh-default-dark"
local LIGHT_THEME = "mesh-default-light"
local AVAILABLE   = {DARK_THEME, LIGHT_THEME}

local function is_dark_theme(theme_id)
    return type(theme_id) == "string" and string.find(theme_id, "dark", 1, true) ~= nil
end

local function configured_theme()
    local cfg = mesh.config()
    if type(cfg.current_theme) == "string" and cfg.current_theme ~= "" then
        return cfg.current_theme
    end
    return DARK_THEME
end

local current_theme = configured_theme()
local current_is_dark = is_dark_theme(current_theme)

state = {
    current   = current_theme,
    is_dark   = current_is_dark,
    available = AVAILABLE,
}

function init()
    mesh.service.set_poll_interval(300000)
end

local function emit_state()
    state = {
        current   = current_theme,
        is_dark   = current_is_dark,
        available = AVAILABLE,
    }
end

function on_poll()
    emit_state()
end

-- Called by the shell after it applies a SetTheme request so that the
-- emitted service state stays in sync with the real active theme.
function on_command_set_current()
    local payload = mesh.service.payload()
    local new_id = payload.theme_id or payload.current
    if type(new_id) == "string" and new_id ~= "" then
        current_theme = new_id
    end
    if type(payload.is_dark) == "boolean" then
        current_is_dark = payload.is_dark
    else
        current_is_dark = is_dark_theme(current_theme)
    end
    emit_state()
    return { ok = true }
end
