-- Hyprland compositor backend.
--
-- Reads workspace and window state from Hyprland's request socket
-- (.socket.sock) and subscribes to its event socket (.socket2.sock) through
-- the host's native socket primitives, so the idle path spawns no processes.
-- Without the `net.socket` capability it falls back to hyprctl queries and a
-- socat/nc event stream. A safety-net poll covers events the stream misses.

state = {
    available = false,
    active_workspace = 1,
    active_window = nil,
    workspaces = {},
}

local stream_active = false
-- Program label of the active event stream (native socket, socat, or nc).
local stream_program = nil
-- Directory holding Hyprland's sockets once located.
local socket_dir = nil

-- Poll intervals: a fast fallback when no event stream is attached, and a
-- slow safety net once events arrive on their own.
local FALLBACK_POLL_MS = 500
local SAFETY_POLL_MS = 30000

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

-- Run one hyprctl-style JSON query. The request socket answers without a
-- process spawn; hyprctl remains the fallback for hosts without `net.socket`.
local function query(name)
    if socket_dir then
        local reply = mesh.socket_request(socket_dir .. "/.socket.sock", "j/" .. name)
        if reply.success then
            return { success = true, stdout = reply.response }
        end
    end
    return mesh.exec("hyprctl", { name, "-j" })
end

local function read_state()
    local ws_result = query("activeworkspace")
    if not ws_result.success then
        return nil
    end

    local win_result = query("activewindow")
    local wss_result = query("workspaces")

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

local function shell_value(script)
    local result = mesh.exec("sh", { "-c", script })
    if result.success then
        local value = trim(result.stdout or "")
        if value ~= "" then
            return value
        end
    end
    return nil
end

-- Candidate socket directories for the running instance. Hyprland >=0.40
-- uses $XDG_RUNTIME_DIR/hypr/<sig>; older versions use /tmp/hypr/<sig>.
local function socket_dir_candidates()
    local candidates = {}
    local sig = shell_value("printf '%s' \"$HYPRLAND_INSTANCE_SIGNATURE\"")
    if not sig then
        local result = mesh.exec("hyprctl", { "instances", "-j" })
        if result.success then
            sig = (result.stdout or ""):match('"instance"%s*:%s*"([^"]+)"')
        end
    end
    if not sig or sig == "" then
        return candidates
    end
    local xdg = shell_value("printf '%s' \"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}\"")
    if xdg then
        candidates[#candidates + 1] = xdg .. "/hypr/" .. sig
    end
    candidates[#candidates + 1] = "/tmp/hypr/" .. sig
    return candidates
end

local function socket_exists(path)
    local test = mesh.exec("sh", { "-c", 'test -S "$1" && echo ok', "sh", path })
    return test.success and trim(test.stdout or "") == "ok"
end

local function stream_label(started, fallback)
    return type(started) == "table" and started.program or fallback
end

-- Attach the event stream: the native socket first, then socat, then nc.
-- Returns the stream's program label, or nil.
local function try_start_stream(dir)
    local socket = dir .. "/.socket2.sock"
    local started = mesh.socket_stream(socket)
    if started then
        socket_dir = dir
        log_info("subscribed natively: " .. socket)
        return stream_label(started, "socket:" .. socket)
    end

    if not socket_exists(socket) then
        return nil
    end

    if mesh.service.can_exec("socat", { "-", "UNIX-CONNECT:" .. socket }) then
        started = mesh.exec_stream("socat", { "-", "UNIX-CONNECT:" .. socket })
        if started then
            log_info("subscribed via socat: " .. socket)
            return stream_label(started, "socat")
        end
        log_warn("socat spawn failed for: " .. socket)
    end

    if mesh.service.can_exec("nc", { "-U", socket }) then
        started = mesh.exec_stream("nc", { "-U", socket })
        if started then
            log_info("subscribed via nc: " .. socket)
            return stream_label(started, "nc")
        end
        log_warn("nc spawn failed for: " .. socket)
    end

    return nil
end

function start(self)
    mesh.service.set_poll_interval(FALLBACK_POLL_MS)

    for _, dir in ipairs(socket_dir_candidates()) do
        stream_program = try_start_stream(dir)
        if stream_program then
            break
        end
    end

    if stream_program then
        stream_active = true
        mesh.service.set_poll_interval(SAFETY_POLL_MS)
    else
        log_debug("no Hyprland event stream available; using 500 ms poll")
    end

    refresh_state(self)
end

-- The event stream ended (compositor restart or socket error). Return to the
-- fast poll so state keeps flowing; the supervisor restarts the provider when
-- it fails outright.
function on_stream_closed(self, program, _success)
    if program ~= stream_program then
        return
    end
    log_warn("event stream closed; falling back to 500 ms poll")
    stream_active = false
    stream_program = nil
    socket_dir = nil
    mesh.service.set_poll_interval(FALLBACK_POLL_MS)
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

-- Handle one socket2 line at a time. Workspace changes are ordered state
-- transitions, not just a final snapshot: keeping the callback line-oriented
-- lets the backend publish every hop in a rapid sequence. The shell can still
-- batch ordinary provider updates, while the WorkspaceChanged event emitted
-- for each hop remains an ordering barrier for this stream.
function on_stream_line(self, program, line)
    if program ~= stream_program then
        return
    end

    local event, payload = line:match("^([%a%d]+)>>(.*)$")
    if not event then
        return
    end

    local active_window = nil
    local has_active_window_payload = false
    local active_workspace = nil
    local needs_active_window_query = false
    local needs_active_workspace_query = false
    local needs_workspace_list_query = false

    if event == "activewindow" then
        active_window, has_active_window_payload = parse_active_window_event(payload)
        needs_active_window_query = false
    elseif event == "activewindowv2" then
        -- The v2 event carries only the address. Query just this field when
        -- there is no richer activewindow line to use.
        needs_active_window_query = true
    elseif event == "workspacev2" then
        active_workspace = parse_workspace_v2_event(payload)
        needs_active_workspace_query = active_workspace == nil
    elseif event == "workspace" then
        -- Legacy payloads carry a workspace name, commonly the numeric id.
        active_workspace = tonumber(payload)
        needs_active_workspace_query = active_workspace == nil
    elseif event == "focusedmon" then
        local workspace_name = payload:match("^[^,]*,(.*)$") or ""
        active_workspace = tonumber(workspace_name)
        needs_active_workspace_query = active_workspace == nil
    elseif WORKSPACE_LIST_EVENTS[event] then
        needs_workspace_list_query = true
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
        local result = query("activewindow")
        if result.success then
            next_state.active_window = parse_active_window(result.stdout or "")
            next_state.available = true
        end
    end

    if active_workspace ~= nil then
        next_state.active_workspace = active_workspace
        next_state.available = true
    elseif needs_active_workspace_query then
        local result = query("activeworkspace")
        if result.success then
            local parsed = parse_active_workspace(result.stdout or "")
            if parsed then
                next_state.active_workspace = parsed
                next_state.available = true
            end
        end
    end

    if needs_workspace_list_query then
        local result = query("workspaces")
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
