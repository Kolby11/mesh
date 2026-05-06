state = {
    current = "mesh-default-dark",
    theme_id = "mesh-default-dark",
    is_dark = true,
}

local function derive_is_dark(theme_id, payload)
    if payload.is_dark ~= nil then
        return payload.is_dark == true
    end

    return type(theme_id) == "string" and theme_id:find("dark", 1, true) ~= nil
end

local function sync_theme(theme_id, payload)
    local current = theme_id or "mesh-default-dark"
    state = {
        current = current,
        theme_id = current,
        is_dark = derive_is_dark(current, payload or {}),
    }
end

function init()
    mesh.service.set_poll_interval(5000)
    local config = mesh.config()
    sync_theme(config.current_theme or config.theme_id or state.current, {})
end

function on_poll()
    state = state
end

function on_command_set_current()
    local payload = mesh.service.payload()
    sync_theme(payload.current or payload.theme_id or state.current, payload)
end
