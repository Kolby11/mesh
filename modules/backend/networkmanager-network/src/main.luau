state = {
    available = false,
    wifi_enabled = false,
    connections = {},
    devices = {},
    networks = {},
}

function init()
    mesh.service.set_poll_interval(1000)
end

function on_poll()
    state = state
end

function on_command_set_wifi_enabled()
    local payload = mesh.service.payload()
    state = {
        available = state.available,
        wifi_enabled = payload.enabled == true,
        connections = state.connections,
        devices = state.devices,
        networks = state.networks,
    }
    return { ok = true }
end
