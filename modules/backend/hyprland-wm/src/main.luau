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

local stream_active = false
-- Program name actually used for the stream (socat or nc).
local stream_program = nil

local function log_info(msg)
    mesh.log.info("[hyprland-wm] " .. msg)
end

local function log_debug(msg)
    mesh.log.debug("[hyprland-wm] " .. msg)
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

local function windows_equal(a, b)
    if a == nil or b == nil then
        return a == b
    end
    return a.title == b.title and a.class == b.class
end

local function workspaces_equal(a, b)
    if #a ~= #b then
        return false
    end
    for index = 1, #a do
        local left = a[index]
        local right = b[index]
        if left.id ~= right.id or left.name ~= right.name or left.windows ~= right.windows then
            return false
        end
    end
    return true
end

local function commit_state(self, next_state)
    local previous = state
    local availability_changed = previous.available ~= next_state.available
    local workspace_changed = previous.active_workspace ~= next_state.active_workspace
    local window_changed = not windows_equal(previous.active_window, next_state.active_window)
    local workspace_list_changed = not workspaces_equal(previous.workspaces, next_state.workspaces)

    state = next_state
    if not (availability_changed or workspace_changed or window_changed or workspace_list_changed) then
        return
    end

    local win_title = state.active_window and state.active_window.title or "(desktop)"
    local message = string.format(
        "state available=%s ws=%d window=%s",
        tostring(state.available),
        state.active_workspace,
        win_title
    )
    if window_changed and not (availability_changed or workspace_changed or workspace_list_changed) then
        -- Some applications animate their title at 10 Hz or faster. Keep the
        -- state reactive without turning cosmetic churn into an info-log storm.
        log_debug(message)
    else
        log_info(message)
    end

    if self and state.available then
        if availability_changed or workspace_changed then
            self.WorkspaceChanged:fire({
                id = state.active_workspace,
                name = tostring(state.active_workspace),
            })
        end
        if (availability_changed or window_changed) and state.active_window then
            self.WindowChanged:fire({
                title = state.active_window.title,
                class = state.active_window.class,
            })
        end
    end
end

local function refresh_state(self)
    commit_state(self, read_state() or {
        available = false,
        active_workspace = 1,
        active_window = nil,
        workspaces = {},
    })
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
-- Returns the program name if a stream handle was started, or nil.
local function try_start_stream(socket)
    if mesh.service.can_exec("socat", { "-", "UNIX-CONNECT:" .. socket }) then
        local started = mesh.exec_stream("socat", { "-", "UNIX-CONNECT:" .. socket })
        if started then
            log_info("subscribed via socat: " .. socket)
            return type(started) == "table" and started.program or "socat"
        end
        log_warn("socat spawn failed for: " .. socket)
    end

    if mesh.service.can_exec("nc", { "-U", socket }) then
        -- -q 1 tells nc to quit 1 second after EOF, which keeps the process
        -- alive while the socket streams. Ignored harmlessly by ncat/openbsd-nc.
        local started = mesh.exec_stream("nc", { "-U", socket })
        if started then
            log_info("subscribed via nc: " .. socket)
            return type(started) == "table" and started.program or "nc"
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
            log_debug("no stream tool available (socat/nc); using 500 ms poll")
        end
    else
        log_debug("could not locate Hyprland event socket; using 500 ms poll")
    end

    refresh_state(self)
end

function on_poll(self)
    refresh_state(self)
end

-- Hyprland socket2 event lines: `eventname>>params`
local WORKSPACE_LIST_EVENTS = {
    createworkspace = true,
    createworkspacev2 = true,
    destroyworkspace = true,
    destroyworkspacev2 = true,
    openwindow = true,
    closewindow = true,
    movewindow = true,
    movewindowv2 = true,
    moveworkspace = true,
    moveworkspacev2 = true,
}

local function parse_active_window_event(payload)
    local class, title = payload:match("^(.-),(.*)$")
    if not class then
        return nil, false
    end
    if class == "" and title == "" then
        return nil, true
    end
    return { class = class, title = title }, true
end

local function parse_workspace_v2_event(payload)
    local id = tonumber(payload:match("^(%-?%d+),"))
    return id
end

function on_stream_batch(self, program, lines)
    if program ~= stream_program then
        return
    end

    local active_window = nil
    local has_active_window_payload = false
    local active_workspace = nil
    local needs_active_window_query = false
    local needs_active_workspace_query = false
    local needs_workspace_list_query = false

    for _, line in ipairs(lines) do
        local event, payload = line:match("^([%a%d]+)>>(.*)$")
        if event == "activewindow" then
            active_window, has_active_window_payload = parse_active_window_event(payload)
            needs_active_window_query = false
        elseif event == "activewindowv2" and not has_active_window_payload then
            -- The v2 event carries only the address. Most batches also carry
            -- `activewindow`; query just this field when they do not.
            needs_active_window_query = true
        elseif event == "workspacev2" then
            active_workspace = parse_workspace_v2_event(payload)
            needs_active_workspace_query = active_workspace == nil
        elseif event == "workspace" then
            -- Legacy payloads carry a workspace name, commonly the numeric id.
            active_workspace = tonumber(payload)
            needs_active_workspace_query = active_workspace == nil
        elseif event == "focusedmon" and active_workspace == nil then
            local workspace_name = payload:match("^[^,]*,(.*)$") or ""
            active_workspace = tonumber(workspace_name)
            needs_active_workspace_query = active_workspace == nil
        elseif event and WORKSPACE_LIST_EVENTS[event] then
            needs_workspace_list_query = true
        end
    end

    if not (has_active_window_payload
        or active_workspace ~= nil
        or needs_active_window_query
        or needs_active_workspace_query
        or needs_workspace_list_query) then
        return
    end

    local next_state = {
        available = state.available,
        active_workspace = state.active_workspace,
        active_window = state.active_window,
        workspaces = state.workspaces,
    }

    if has_active_window_payload then
        next_state.active_window = active_window
        next_state.available = true
    elseif needs_active_window_query then
        local result = mesh.exec("hyprctl", { "activewindow", "-j" })
        if result.success then
            next_state.active_window = parse_active_window(result.stdout or "")
            next_state.available = true
        end
    end

    if active_workspace ~= nil then
        next_state.active_workspace = active_workspace
        next_state.available = true
    elseif needs_active_workspace_query then
        local result = mesh.exec("hyprctl", { "activeworkspace", "-j" })
        if result.success then
            local parsed = parse_active_workspace(result.stdout or "")
            if parsed then
                next_state.active_workspace = parsed
                next_state.available = true
            end
        end
    end

    if needs_workspace_list_query then
        local result = mesh.exec("hyprctl", { "workspaces", "-j" })
        if result.success then
            next_state.workspaces = parse_workspaces(result.stdout or "")
            next_state.available = true
        end
    end

    commit_state(self, next_state)
end

function on_command_focus_workspace(self)
    local payload = mesh.service.payload()
    local id = tonumber(payload.id)
    if not id then
        return { ok = false, error = "missing workspace id" }
    end
    log_info("focus_workspace(" .. tostring(id) .. ")")
    local workspace = tostring(id)
    local dispatcher = 'hl.dsp.focus({ workspace = "' .. workspace .. '" })'
    local result = mesh.exec("hyprctl", { "dispatch", dispatcher })

    -- Hyprland 0.55 replaced the legacy dispatcher arguments with Lua
    -- dispatcher expressions. Retain the old form as a compatibility fallback
    -- for installations that have not moved to the Lua command API yet.
    if not result.success then
        result = mesh.exec("hyprctl", { "dispatch", "workspace", workspace })
    end

    if not result.success then
        log_warn("workspace dispatch failed: " .. trim(result.stderr))
    end

    -- Do not synchronously refresh here. The dispatch can return before
    -- Hyprland exposes the new active workspace, so an immediate query may
    -- publish the old workspace and make the UI switch back. The event stream
    -- (with the polling fallback) supplies the authoritative update.
    return { ok = result.success, error = result.stderr }
end
