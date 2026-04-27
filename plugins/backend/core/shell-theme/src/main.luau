-- Shell theme service backend.
-- Tracks available themes and the currently active theme.
-- The shell calls on_command_set_current whenever the active theme changes
-- so this backend stays in sync without polling the filesystem.

mesh.service.set_poll_interval(300000)

local DARK_THEME  = "mesh-default-dark"
local LIGHT_THEME = "mesh-default-light"
local AVAILABLE   = {DARK_THEME, LIGHT_THEME}

local current_theme = DARK_THEME

local function emit_state()
    mesh.service.emit({
        current   = current_theme,
        is_dark   = (current_theme == DARK_THEME),
        available = AVAILABLE,
    })
end

function on_poll()
    emit_state()
end

-- Called by the shell after it applies a SetTheme request so that the
-- emitted service state stays in sync with the real active theme.
function on_command_set_current()
    local new_id = __payload_theme_id
    if type(new_id) == "string" and new_id ~= "" then
        current_theme = new_id
    end
    emit_state()
end
