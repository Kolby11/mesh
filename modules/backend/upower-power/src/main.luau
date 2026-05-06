state = {
    available = false,
    percent = 0,
    charging = false,
}

function init()
    mesh.service.set_poll_interval(5000)
end

function on_poll()
    state = state
end
