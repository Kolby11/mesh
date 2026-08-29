state = {
    current = "tokyo-night",
    theme_id = "tokyo-night",
    mode = "default",
    color_scheme = "unknown",
    contrast = "normal",
    is_dark = false,
    tokens = {},
    provenance = {},
    revision = "0000000000000000",
    durable_revision = "shared:0",
    modes = {},
    themes = {},
    available = {},
}

local function derive_is_dark(payload)
    if payload.is_dark ~= nil then
        return payload.is_dark == true
    end

    return type(payload.color_scheme) == "string"
        and payload.color_scheme:lower() == "dark"
end

local function sync_theme(theme_id, payload)
    local current = theme_id or "tokyo-night"
    local source = payload or {}
    local themes = source.themes or state.themes or {}
    local available = source.available or state.available or {}
    state = {
        current = current,
        theme_id = current,
        mode = source.mode or state.mode,
        color_scheme = source.color_scheme or state.color_scheme,
        contrast = source.contrast or state.contrast,
        is_dark = derive_is_dark(source),
        tokens = source.tokens or state.tokens,
        provenance = source.provenance or state.provenance,
        revision = source.revision or state.revision,
        durable_revision = source.durable_revision or state.durable_revision,
        modes = source.modes or state.modes,
        themes = themes,
        available = available,
    }
end

function start(self)
    mesh.service.set_poll_interval(5000)
    local config = mesh.config()
    local shell = config.__shell or {}
    sync_theme(shell.theme or config.theme_id or state.current, {
        mode = shell.theme_mode,
        color_scheme = shell.theme_color_scheme,
        contrast = shell.theme_contrast,
        is_dark = shell.theme_is_dark,
    })
end

function on_poll(self)
    state = state
end

function on_command_set_current(self)
    local payload = mesh.service.payload()
    sync_theme(payload.current or payload.theme_id or state.current, payload)
end
