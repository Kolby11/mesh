state = {
    available = false,
    percent = 0,
    charging = false,
}

function start(self)
    mesh.service.set_poll_interval(5000)
end

function on_poll(self)
    state = state
end
