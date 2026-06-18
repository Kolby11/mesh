-- Hyprland compositor backend.
--
-- Reads workspace and window state via hyprctl. Subscribes to Hyprland's
-- IPC event socket (.socket2.sock) for instant updates. socat is tried
-- first; nc is tried as a fallback. A safety-net poll covers any events
-- the stream misses or events that arrive before the stream connects.

state = {
    available = false,
    active_workspace = 1,
    active_window = nil,
    workspaces = {},
}

local last_snapshot = nil
local stream_active = false
-- Program name actually used for the stream (socat or nc).
local stream_program = nil

local function log_info(msg)
    mesh.log.info("[hyprland-wm] " .. msg)
end

local function log_warn(msg)
    mesh.log.warn("[hyprland-wm] " .. msg)
end

local function trim(s)
    return (s or ""):match("^%s*(.-)%s*$")
end

local function parse_active_workspace(stdout)
    local id = stdout:match('"id"%s*:%s*(%d+)')
    return tonumber(id)
end

local function parse_active_window(stdout)
    local class = stdout:match('"class"%s*:%s*"(.-[^\\])"') or ""
    local title = stdout:match('"title"%s*:%s*"(.-[^\\])"') or ""
    title = title:gsub('\\"', '"'):gsub("\\\\", "\\"):gsub("\\n", " ")
    class = class:gsub('\\"', '"'):gsub("\\\\", "\\")
    if class == "" and title == "" then
        return nil
    end
    return { title = title, class = class }
end

local function parse_workspaces(stdout)
    local list = {}
    for entry in stdout:gmatch("{([^}]+)}") do
        local id = tonumber(entry:match('"id"%s*:%s*(%d+)'))
        local name = entry:match('"name"%s*:%s*"([^"]*)"') or ""
        local windows = tonumber(entry:match('"windows"%s*:%s*(%d+)')) or 0
        if id then
            list[#list + 1] = { id = id, name = name, windows = windows }
        end
    end
    table.sort(list, function(a, b) return a.id < b.id end)
    return list
end

local function read_state()
    local ws_result = mesh.exec("hyprctl", { "activeworkspace", "-j" })
    if not ws_result.success then
        return nil
    end

    local win_result = mesh.exec("hyprctl", { "activewindow", "-j" })
    local wss_result = mesh.exec("hyprctl", { "workspaces", "-j" })

    local active_workspace = parse_active_workspace(ws_result.stdout or "")
    if not active_workspace then
        return nil
    end

    local active_window = nil
    if win_result.success then
        active_window = parse_active_window(win_result.stdout or "")
    end

    local workspaces = {}
    if wss_result.success then
        workspaces = parse_workspaces(wss_result.stdout or "")
    end

    return {
        available = true,
        active_workspace = active_workspace,
        active_window = active_window,
        workspaces = workspaces,
    }
end

local function refresh_state(self)
    local next_state = read_state()
    if next_state then
        state = next_state
    else
        state = { available = false, active_workspace = 1, active_window = nil, workspaces = {} }
    end

    local win_title = state.active_window and state.active_window.title or "(desktop)"
    local snapshot = string.format(
        "available=%s ws=%d window=%s",
        tostring(state.available),
        state.active_workspace,
        win_title
    )

    if snapshot ~= last_snapshot then
        last_snapshot = snapshot
        log_info("state " .. snapshot)

        if self and state.available then
            self.WorkspaceChanged:fire({
                id = state.active_workspace,
                name = tostring(state.active_workspace),
            })
            if state.active_window then
                self.WindowChanged:fire({
                    title = state.active_window.title,
                    class = state.active_window.class,
                })
            end
        end
    end
end

-- Discover the Hyprland IPC event socket path.
-- Tries $HYPRLAND_INSTANCE_SIGNATURE via sh first (zero extra hyprctl calls
-- when the env var is set), then falls back to `hyprctl instances -j`.
local function find_event_socket()
    -- Preferred: read the env var that Hyprland sets for the running session.
    local env_result = mesh.exec("sh", { "-c", "printf '%s' \"$HYPRLAND_INSTANCE_SIGNATURE\"" })
    if env_result.success then
        local sig = trim(env_result.stdout or "")
        if sig ~= "" then
            -- Hyprland ≥0.40 uses $XDG_RUNTIME_DIR/hypr/<sig>/.socket2.sock;
            -- older versions use /tmp/hypr/<sig>/.socket2.sock. Try both.
            local xdg_result = mesh.exec("sh", { "-c", "printf '%s' \"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}\"" })
            if xdg_result.success then
                local xdg = trim(xdg_result.stdout or "")
                if xdg ~= "" then
                    local candidate = xdg .. "/hypr/" .. sig .. "/.socket2.sock"
                    local test = mesh.exec("sh", { "-c", "test -S " .. candidate .. " && echo ok" })
                    if test.success and trim(test.stdout or "") == "ok" then
                        return candidate
                    end
                end
            end
            local candidate = "/tmp/hypr/" .. sig .. "/.socket2.sock"
            local test = mesh.exec("sh", { "-c", "test -S " .. candidate .. " && echo ok" })
            if test.success and trim(test.stdout or "") == "ok" then
                return candidate
            end
        end
    end

    -- Fallback: parse `hyprctl instances -j`.
    local result = mesh.exec("hyprctl", { "instances", "-j" })
    if not result.success then
        return nil
    end
    local instance = (result.stdout or ""):match('"instance"%s*:%s*"([^"]+)"')
    if not instance or instance == "" then
        return nil
    end
    return "/tmp/hypr/" .. instance .. "/.socket2.sock"
end

-- Try to attach a stream subprocess that reads events from the socket.
-- socat is tried first because it handles bidirectional UNIX sockets cleanly.
-- nc -U is tried as a fallback (available on most distros as part of nmap
-- or netcat-openbsd, and does not need a bidirectional link for read-only use).
-- Returns the program name if a stream was started, or nil.
local function try_start_stream(socket)
    if mesh.service.has_capability("exec.socat") then
        local started = mesh.exec_stream("socat", { "-", "UNIX-CONNECT:" .. socket })
        if started then
            log_info("subscribed via socat: " .. socket)
            return "socat"
        end
        log_warn("socat spawn failed for: " .. socket)
    end

    if mesh.service.has_capability("exec.nc") then
        -- -q 1 tells nc to quit 1 second after EOF, which keeps the process
        -- alive while the socket streams. Ignored harmlessly by ncat/openbsd-nc.
        local started = mesh.exec_stream("nc", { "-U", socket })
        if started then
            log_info("subscribed via nc: " .. socket)
            return "nc"
        end
        log_warn("nc spawn failed for: " .. socket)
    end

    return nil
end

function start(self)
    -- Start with a fast safety-net poll; relax to 2 s when the stream is up.
    mesh.service.set_poll_interval(500)

    local socket = find_event_socket()
    if socket then
        stream_program = try_start_stream(socket)
        if stream_program then
            stream_active = true
            mesh.service.set_poll_interval(2000)
        else
            log_warn("no stream tool available (socat/nc); using 500 ms poll")
        end
    else
        log_warn("could not locate Hyprland event socket; using 500 ms poll")
    end

    refresh_state(self)
end

function on_poll(self)
    refresh_state(self)
end

-- Hyprland socket2 event lines: `eventname>>params`
-- Full list of workspace-affecting events we react to:
--   workspace / workspacev2          — active workspace switched
--   focusedmon                       — monitor (and thus workspace) focus changed
--   createworkspace / v2             — new workspace created
--   destroyworkspace / v2            — workspace removed
--   openwindow                       — window opened (window count changes)
--   closewindow                      — window closed (window count changes)
--   activewindow / activewindowv2    — focused window title/class changed
--   movewindow / movewindowv2        — window moved between workspaces
--   moveworkspace / moveworkspacev2  — workspace moved to another monitor
local WORKSPACE_EVENTS = {
    workspace = true,
    workspacev2 = true,
    focusedmon = true,
    createworkspace = true,
    createworkspacev2 = true,
    destroyworkspace = true,
    destroyworkspacev2 = true,
    openwindow = true,
    closewindow = true,
    activewindow = true,
    activewindowv2 = true,
    movewindow = true,
    movewindowv2 = true,
    moveworkspace = true,
    moveworkspacev2 = true,
}

function on_stream_batch(self, program, lines)
    if program ~= stream_program then
        return
    end

    local needs_refresh = false
    for _, line in ipairs(lines) do
        local event = line:match("^([%a%d]+)>>")
        if event and WORKSPACE_EVENTS[event] then
            needs_refresh = true
            break
        end
    end

    if needs_refresh then
        refresh_state(self)
    end
end

function on_command_switch_workspace(self)
    local payload = mesh.service.payload()
    local id = tonumber(payload.id)
    if not id then
        return { ok = false, error = "missing workspace id" }
    end
    log_info("switch_workspace(" .. tostring(id) .. ")")
    local result = mesh.exec("hyprctl", { "dispatch", "workspace", tostring(id) })
    refresh_state(self)
    return { ok = result.success, error = result.stderr }
end
