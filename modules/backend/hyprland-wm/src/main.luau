-- Hyprland compositor backend.
--
-- Workspace records come from the compositor's ext-workspace protocol.
-- Hyprland's request socket (.socket.sock) adds window counts and monitor
-- names, and its event socket (.socket2.sock) reports focus and window
-- changes, so the idle path spawns no processes. Without `net.socket` the
-- active window falls back to hyprctl and the event stream to socat/nc;
-- window counts are then unavailable. A safety-net poll covers events the
-- streams miss.

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
-- Program label of the ext-workspace stream, and whether its first batch
-- has arrived.
local ext_program = nil
local ext_ready = false
-- ext-workspace handles by connection-local id: name, coordinates, state.
local ext_handles = {}
-- Per Hyprland workspace id: window count and monitor from the request socket.
local details = {}

local EXT_ACTIVE = 1
local EXT_URGENT = 2
local EXT_HIDDEN = 4

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

local function decode_record(source)
    local ok, value = pcall(mesh.json.decode, source)
    if ok and type(value) == "table" then
        return value
    end
    return nil
end

local function parse_active_workspace(stdout)
    local workspace = decode_record(stdout)
    local id = workspace and workspace.id
    if type(id) == "number" and id ~= 0 and id % 1 == 0 then
        return id
    end
    return nil
end

local function parse_active_window(stdout)
    local window = decode_record(stdout)
    if window == nil then
        return nil
    end
    local class = type(window.class) == "string" and window.class or ""
    local title = type(window.title) == "string" and window.title:gsub("[\r\n]", " ") or ""
    if class == "" and title == "" then
        return nil
    end
    return { title = title, class = class }
end

local function is_special(name)
    return name == "special" or name:match("^special:") ~= nil
end

local function parse_workspaces(stdout)
    local records = decode_record(stdout)
    local list = {}
    for _, entry in records or {} do
        if type(entry) ~= "table" then
            continue
        end
        local id = entry.id
        local name = type(entry.name) == "string" and entry.name or tostring(id)
        local windows = type(entry.windows) == "number" and entry.windows or 0
        if type(id) == "number" and id ~= 0 and id % 1 == 0 and not is_special(name) then
            list[#list + 1] = {
                id = id, name = name, windows = math.max(0, math.floor(windows)),
                index = id > 0 and id or nil,
                output = type(entry.monitor) == "string" and entry.monitor or nil,
            }
        end
    end
    table.sort(list, function(a, b) return a.id < b.id end)
    return list
end

-- Run one JSON query on the request socket, without a process spawn.
-- Returns the reply text, or nil without `net.socket` or a located socket.
local function socket_query(name)
    if not socket_dir then
        return nil
    end
    local reply = mesh.socket_request(socket_dir .. "/.socket.sock", "j/" .. name)
    if reply.success then
        return reply.response
    end
    return nil
end

local function query_active_window()
    local reply = socket_query("activewindow")
    if reply then
        return { success = true, stdout = reply }
    end
    return mesh.exec("hyprctl", { "activewindow", "-j" })
end

local function query_active_workspace()
    local reply = socket_query("activeworkspace")
    return reply and parse_active_workspace(reply)
end

-- Reads Hyprland's workspace list and refreshes the per-workspace details.
-- Returns the list, or nil when the request socket is unavailable.
local function query_workspaces()
    local reply = socket_query("workspaces")
    if not reply then
        return nil
    end
    local list = parse_workspaces(reply)
    details = {}
    for _, workspace in list do
        details[workspace.id] = { windows = workspace.windows, output = workspace.output }
    end
    return list
end

-- Hyprland reports each workspace's ID as its only ext-workspace coordinate,
-- as an unsigned value; named workspaces have negative IDs.
local function hyprland_id(handle)
    local coordinate = handle.coordinates and handle.coordinates[1]
    if type(coordinate) == "number" then
        return coordinate >= 2 ^ 31 and coordinate - 2 ^ 32 or coordinate
    end
    return tonumber(handle.name)
end

-- Builds the workspace list from ext-workspace records. Each output has its
-- own active workspace, so the focused one is the only workspace activated in
-- the latest batch, else the current one if it is still active, else the
-- lowest active ID.
local function ext_snapshot()
    local list = {}
    local activated = nil
    local activated_count = 0
    local current_active = false
    local lowest_active = nil
    for _, handle in ext_handles do
        local id = hyprland_id(handle)
        local name = handle.name
        local flags = handle.state or 0
        if id and id ~= 0 and id % 1 == 0 and name and not is_special(name)
            and bit32.band(flags, EXT_HIDDEN) == 0 then
            local active = bit32.band(flags, EXT_ACTIVE) ~= 0
            local detail = details[id] or {}
            list[#list + 1] = {
                id = id, name = name, windows = detail.windows or 0,
                index = id > 0 and id or nil, output = detail.output,
                visible = active, urgent = bit32.band(flags, EXT_URGENT) ~= 0,
            }
            if active then
                if handle.activated then
                    activated = id
                    activated_count += 1
                end
                current_active = current_active or id == state.active_workspace
                if lowest_active == nil or id < lowest_active then
                    lowest_active = id
                end
            end
        end
        handle.activated = nil
    end
    table.sort(list, function(a, b) return a.id < b.id end)
    if activated_count ~= 1 then
        activated = nil
    end
    local active = activated or (current_active and state.active_workspace) or lowest_active
    return list, active
end

local function read_state()
    local hyprland_list = query_workspaces()
    local active_workspace = query_active_workspace()

    local workspaces = hyprland_list
    if ext_ready then
        local ext_active
        workspaces, ext_active = ext_snapshot()
        active_workspace = active_workspace or ext_active
    end
    if not workspaces or not active_workspace then
        return nil
    end

    local active_window = nil
    local win_result = query_active_window()
    if win_result.success then
        active_window = parse_active_window(win_result.stdout or "")
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
        if left.id ~= right.id or left.name ~= right.name or left.windows ~= right.windows
            or left.index ~= right.index or left.output ~= right.output or left.focused ~= right.focused
            or left.visible ~= right.visible or left.urgent ~= right.urgent then
            return false
        end
    end
    return true
end

local function commit_state(self, next_state)
    local workspaces = {}
    for _, workspace in next_state.workspaces do
        workspaces[#workspaces + 1] = {
            id = workspace.id, name = workspace.name, windows = workspace.windows,
            index = workspace.index, output = workspace.output,
            visible = workspace.visible, urgent = workspace.urgent,
            focused = workspace.id == next_state.active_workspace,
        }
    end
    next_state.workspaces = workspaces
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

    local workspace_stream = mesh.wayland_subscribe("workspace")
    if workspace_stream then
        ext_program = stream_label(workspace_stream, "wayland:workspace")
    else
        log_debug("ext-workspace unavailable; reading workspaces from the request socket")
    end

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
-- it fails outright. A closed ext-workspace stream, including a compositor
-- that does not advertise the protocol, leaves the request socket as the
-- workspace source.
function on_stream_closed(self, program, _success)
    if program == ext_program then
        log_warn("ext-workspace stream closed; reading workspaces from the request socket")
        ext_program = nil
        ext_ready = false
        ext_handles = {}
        refresh_state(self)
        return
    end
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

local function on_ext_record(self, line)
    local record = decode_record(line)
    if record == nil then
        return
    end
    local event = record.event
    local id = record.id
    if event == "added" then
        ext_handles[id] = { state = 0 }
    elseif event == "removed" then
        ext_handles[id] = nil
    elseif event == "done" then
        ext_ready = true
        local workspaces, active = ext_snapshot()
        commit_state(self, {
            available = active ~= nil or state.available,
            active_workspace = active or state.active_workspace,
            active_window = state.active_window,
            workspaces = workspaces,
        })
    else
        local handle = ext_handles[id]
        if handle == nil then
            return
        end
        if event == "name" then
            handle.name = record.value
        elseif event == "coordinates" then
            handle.coordinates = record.value
        elseif event == "state" and type(record.value) == "number" then
            local was_active = bit32.band(handle.state, EXT_ACTIVE) ~= 0
            handle.state = record.value
            if not was_active and bit32.band(record.value, EXT_ACTIVE) ~= 0 then
                handle.activated = true
            end
        end
    end
end

-- Handle one socket2 line at a time. Workspace changes are ordered state
-- transitions, not just a final snapshot: keeping the callback line-oriented
-- lets the backend publish every hop in a rapid sequence. The shell can still
-- batch ordinary provider updates, while the WorkspaceChanged event emitted
-- for each hop remains an ordering barrier for this stream.
function on_stream_line(self, program, line)
    if program == ext_program then
        on_ext_record(self, line)
        return
    end
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
        local result = query_active_window()
        if result.success then
            next_state.active_window = parse_active_window(result.stdout or "")
            next_state.available = true
        end
    end

    if active_workspace ~= nil then
        next_state.active_workspace = active_workspace
        next_state.available = true
    elseif needs_active_workspace_query then
        local parsed = query_active_workspace()
        if parsed then
            next_state.active_workspace = parsed
            next_state.available = true
        end
    end

    if needs_workspace_list_query then
        local hyprland_list = query_workspaces()
        if ext_ready then
            next_state.workspaces = ext_snapshot()
        elseif hyprland_list then
            next_state.workspaces = hyprland_list
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
    if id <= 0 then
        workspace = nil
        for _, entry in state.workspaces do
            if entry.id == id then
                workspace = "name:" .. entry.name
                break
            end
        end
        if workspace == nil then
            return { ok = false, error = "unknown named workspace id" }
        end
    end
    local dispatcher = 'hl.dsp.focus({ workspace = ' .. string.format("%q", workspace) .. ' })'
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
