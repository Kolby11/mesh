-- Hyprland applications provider.
--
-- Pinned apps are XDG desktop entries; running state comes from Hyprland
-- clients matched by window class. Window open/close/focus events on the
-- socket2 stream trigger a client re-read, so the idle path spawns nothing.
-- Every launchable entry is indexed once, in a task after start, for
-- launcher grids.

local DEFAULT_PINNED = { "kitty", "firefox", "zen", "vesktop", "spotify", "code", "steam", "blender" }
local FALLBACK_POLL_MS = 2000
local SAFETY_POLL_MS = 30000

state = { available = false, apps = {}, installed = {} }

local data_dirs = nil
-- Desktop id -> parsed entry (or false when no entry exists).
local entries = {}
-- Lowercased window class -> desktop id.
local class_index = {}
local socket_dir = nil
-- App id -> addresses of its windows, most recently focused first.
local windows_by_app = {}

local function shell_value(script)
    local result = mesh.exec("sh", { "-c", script })
    local value = result.success and (result.stdout or "") or ""
    return value ~= "" and value or nil
end

local function resolve_data_dirs()
    if data_dirs then
        return data_dirs
    end
    data_dirs = {}
    local joined = shell_value("printf '%s' \"${XDG_DATA_HOME:-$HOME/.local/share}:$HOME/.nix-profile/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}\"") or "/usr/share"
    for dir in joined:gmatch("[^:]+") do
        if dir:sub(1, 1) == "/" then
            data_dirs[#data_dirs + 1] = dir .. "/applications"
        end
    end
    return data_dirs
end

-- Icon theme lookup order mirrors a typical XDG desktop: the Papirus family,
-- Adwaita, then hicolor and pixmaps. Results are cached per icon name.
local ICON_LOOKUP = "for b in \"${XDG_DATA_HOME:-$HOME/.local/share}\" \"$HOME/.nix-profile/share\" $(printf %s \"${XDG_DATA_DIRS:-/usr/local/share:/usr/share}\" | tr : \" \"); do for t in Papirus-Light Papirus Papirus-Dark Adwaita hicolor; do for s in scalable 256x256 128x128 96x96 64x64 48x48; do for c in apps categories; do for e in svg png; do f=\"$b/icons/$t/$s/$c/$1.$e\"; [ -f \"$f\" ] && { printf %s \"$f\"; exit 0; }; done; done; done; done; for e in svg png; do f=\"$b/pixmaps/$1.$e\"; [ -f \"$f\" ] && { printf %s \"$f\"; exit 0; }; done; done; exit 1"
local icon_paths = {}

-- Resolve an Icon= value to an absolute file. Docks show full-colour app art,
-- so they need the file itself rather than a semantic (tintable) icon name.
local function icon_path(icon)
    if icon == nil or icon == "" then
        return ""
    end
    if icon:sub(1, 1) == "/" then
        return icon
    end
    if icon_paths[icon] == nil then
        local result = mesh.exec("sh", { "-c", ICON_LOOKUP, "sh", icon })
        icon_paths[icon] = result.success and (result.stdout or "") or ""
    end
    return icon_paths[icon]
end

local function parse_desktop(contents)
    local entry, in_main = {}, false
    for line in contents:gmatch("[^\n]+") do
        local section = line:match("^%[(.+)%]$")
        if section then
            in_main = section == "Desktop Entry"
        elseif in_main then
            local key, value = line:match("^([%w%-]+)%s*=%s*(.*)$")
            if key and entry[key] == nil then
                entry[key] = value
            end
        end
    end
    return entry
end

-- Exec with field codes removed; quotes dropped because the launch grant
-- embeds the command in a quoted Lua string.
local function launch_command(exec)
    local command = (exec or ""):gsub("%%[fFuUdDnNickvm]", ""):gsub("%%%%", "%%"):gsub("[\"\\]", "")
    return (command:gsub("%s+", " "):match("^%s*(.-)%s*$"))
end

local function load_entry(id)
    if entries[id] ~= nil then
        return entries[id] or nil
    end
    entries[id] = false
    for _, dir in resolve_data_dirs() do
        local reply = mesh.read_file(dir .. "/" .. id .. ".desktop")
        if reply.success then
            local raw = parse_desktop(reply.contents)
            local entry = {
                id = id,
                name = raw.Name or id,
                icon = raw.Icon or id,
                command = launch_command(raw.Exec),
                class = string.lower(raw.StartupWMClass or id),
                listed = raw.NoDisplay ~= "true" and raw.Hidden ~= "true"
                    and (raw.Type or "Application") == "Application" and (raw.Exec or "") ~= "",
            }
            entries[id] = entry
            class_index[entry.class] = id
            class_index[string.lower(id)] = id
            return entry
        end
    end
    return nil
end

local function pinned_ids()
    local ok, config = pcall(mesh.config)
    local list = ok and type(config) == "table" and config.pinned
    return type(list) == "table" and list or DEFAULT_PINNED
end

local function query(name)
    if socket_dir then
        local reply = mesh.socket_request(socket_dir .. "/.socket.sock", "j/" .. name)
        if reply.success then
            return { success = true, stdout = reply.response }
        end
    end
    return mesh.exec("hyprctl", { name, "-j" })
end

local function decode(result)
    if not result.success then
        return nil
    end
    local ok, value = pcall(mesh.json.decode, result.stdout or "")
    return ok and type(value) == "table" and value or nil
end

local function app_id_for_class(class)
    local lowered = string.lower(class or "")
    if lowered == "" then
        return nil
    end
    if class_index[lowered] then
        return class_index[lowered]
    end
    -- Reverse-DNS classes (org.telegram.desktop) often name their entry.
    if load_entry(class) then
        return class
    end
    if load_entry(lowered) then
        return lowered
    end
    return lowered
end

local LIST_ENTRIES = "for d in \"${XDG_DATA_HOME:-$HOME/.local/share}\" \"$HOME/.nix-profile/share\" $(printf %s \"${XDG_DATA_DIRS:-/usr/local/share:/usr/share}\" | tr : \" \"); do [ -d \"$d/applications\" ] && ls \"$d/applications\"; done; exit 0"
-- Installed entries without running/focus state; refresh() overlays that.
local installed_index = {}

local function index_installed()
    local result = mesh.exec("sh", { "-c", LIST_ENTRIES })
    local seen, list = {}, {}
    for file in (result.stdout or ""):gmatch("[^\n]+") do
        local id = file:match("^(.+)%.desktop$")
        if id and not seen[id] then
            seen[id] = true
            local entry = load_entry(id)
            if entry and entry.listed then
                list[#list + 1] = { id = id, name = entry.name, icon = icon_path(entry.icon) }
            end
        end
    end
    table.sort(list, function(a, b)
        return string.lower(a.name) < string.lower(b.name)
    end)
    installed_index = list
end

local function installed_with(running)
    local list = {}
    for _, app in installed_index do
        local run = running[app.id]
        list[#list + 1] = {
            id = app.id,
            name = app.name,
            icon = app.icon,
            pinned = false,
            running = run ~= nil,
            focused = run ~= nil and run.focused,
            windows = run and run.windows or 0,
        }
    end
    return list
end

local function refresh()
    local clients = decode(query("clients"))
    if clients == nil then
        state = { available = false, apps = {}, installed = installed_with({}) }
        return
    end
    local active = decode(query("activewindow")) or {}

    local running = {}
    local order = {}
    table.sort(clients, function(a, b)
        return (a.focusHistoryID or 1e9) < (b.focusHistoryID or 1e9)
    end)
    windows_by_app = {}
    for _, client in clients do
        local id = app_id_for_class(client.class)
        if id and client.mapped ~= false then
            local record = running[id]
            if not record then
                record = { windows = 0, focused = false }
                running[id] = record
                order[#order + 1] = id
            end
            record.windows += 1
            record.focused = record.focused or client.address == active.address
            local addresses = windows_by_app[id] or {}
            addresses[#addresses + 1] = client.address
            windows_by_app[id] = addresses
        end
    end

    local apps, listed = {}, {}
    local function push(id, pinned)
        if listed[id] then
            return
        end
        listed[id] = true
        local entry = load_entry(id)
        local run = running[id]
        if not entry and not run then
            return
        end
        apps[#apps + 1] = {
            id = id,
            name = entry and entry.name or id,
            icon = icon_path(entry and entry.icon or id),
            pinned = pinned,
            running = run ~= nil,
            focused = run ~= nil and run.focused,
            windows = run and run.windows or 0,
        }
    end
    for _, id in pinned_ids() do
        push(tostring(id), true)
    end
    table.sort(order)
    for _, id in order do
        push(id, false)
    end
    state = { available = true, apps = apps, installed = installed_with(running) }
end

local function try_stream()
    local sig = shell_value("printf '%s' \"$HYPRLAND_INSTANCE_SIGNATURE\"")
    local runtime = shell_value("printf '%s' \"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}\"")
    if not sig or not runtime then
        return false
    end
    local dir = runtime .. "/hypr/" .. sig
    if mesh.socket_stream(dir .. "/.socket2.sock") then
        socket_dir = dir
        return true
    end
    return false
end

function start(self)
    mesh.service.set_poll_interval(try_stream() and SAFETY_POLL_MS or FALLBACK_POLL_MS)
    refresh()
    task.spawn(function()
        index_installed()
        refresh()
    end)
end

local CLIENT_EVENTS = {
    openwindow = true, closewindow = true, activewindowv2 = true,
    movewindowv2 = true, windowtitlev2 = false,
}

function on_stream_line(self, _program, line)
    local event = line:match("^([%a%d]+)>>")
    if event and CLIENT_EVENTS[event] then
        refresh()
    end
end

function on_stream_closed(self, _program, _success)
    socket_dir = nil
    mesh.service.set_poll_interval(FALLBACK_POLL_MS)
end

function on_poll(self)
    refresh()
end

local function dispatch(expression)
    local result = mesh.exec("hyprctl", { "dispatch", expression })
    return { ok = result.success, error = result.stderr }
end

-- Focus the app's most recent window; when it already has focus, cycle to its
-- least recent one. Launch it when no window is open.
function on_command_activate(self)
    local id = tostring(mesh.service.payload().id or "")
    if id == "" then
        return { ok = false, error = "missing app id" }
    end
    local addresses = windows_by_app[id]
    if addresses and #addresses > 0 then
        local focused = false
        for _, app in state.apps do
            if app.id == id then
                focused = app.focused
            end
        end
        local target = focused and addresses[#addresses] or addresses[1]
        return dispatch('hl.dsp.focus({ window = "address:' .. target .. '" })')
    end
    local entry = load_entry(id)
    local command = entry and entry.command or ""
    if command == "" then
        return { ok = false, error = "no desktop entry for " .. id }
    end
    return dispatch('hl.dsp.exec_cmd("' .. command .. '")')
end
