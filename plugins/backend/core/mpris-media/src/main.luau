-- MPRIS media backend placeholder.
-- This keeps the plugin loadable while the real MPRIS integration
-- is still being implemented in Lua.

local warned = false

function init()
    mesh.service.set_poll_interval(30000)
end

function on_poll()
    if not warned then
        mesh.log.warn("MPRIS backend is not implemented yet; reporting unavailable")
        warned = true
    end
    mesh.service.emit_unavailable()
end
