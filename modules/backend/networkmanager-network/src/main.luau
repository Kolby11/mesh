state = {
    available = false,
    wifi_enabled = false,
    connections = {},
    devices = {},
    networks = {},
}

-- Command-driven: without `on_poll` the provider takes no periodic wakeups.
function start(self) end


function on_command_set_wifi_enabled(self)
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
