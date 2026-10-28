-- Watcher and host policy stays in the provider; core transports D-Bus records.
local PATH = "/StatusNotifierWatcher"
local HOST = "org.kde.StatusNotifierHost.mesh"
local MAX_ITEMS = 32
local WATCHERS = { "org.kde.StatusNotifierWatcher", "org.freedesktop.StatusNotifierWatcher" }
local ITEMS = { "org.kde.StatusNotifierItem", "org.freedesktop.StatusNotifierItem" }
local entries, subscriptions, watchers, hosts = {}, {}, {}, {}
local host, names
local host_ready = false
state = { available = false, items = {} }

local function plain(value)
    if type(value) == "table" and value.signature then return plain(value.value) end
    return value
end

local function call(destination, path, interface, member, signature, args)
    local ok, reply = pcall(mesh.dbus_call, "session", {
        destination = destination, path = path, interface = interface, member = member,
        signature = signature or "", args = args or {},
    })
    return ok and reply or { success = false, error = tostring(reply), body = {} }
end

local function owner(name)
    local reply = call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner", "s", { name })
    return reply.success and plain(reply.body[1]) or nil
end

local function properties(destination, path, interface)
    local reply = call(destination, path, "org.freedesktop.DBus.Properties", "GetAll", "s", { interface })
    if not reply.success then return nil end
    local result = {}
    for key, value in pairs(plain(reply.body[1]) or {}) do result[key] = plain(value) end
    return result
end

local function registered()
    local ids = mesh.json.decode("[]")
    for id in pairs(entries) do ids[#ids + 1] = id end
    table.sort(ids)
    return ids
end

local function publish()
    local list = mesh.json.decode("[]")
    local remaining = 48 * 1024
    for _, id in registered() do
        local entry = entries[id]
        if entry.item then
            local item = entry.item
            local weight = 512
            for _, value in pairs(item) do if type(value) == "string" then weight += #value * 2 end end
            if weight <= remaining then list[#list + 1] = item; remaining -= weight end
        end
    end
    state = { available = host_ready, items = list }
    for _, watcher in pairs(watchers) do
        if watcher.local_service and watcher.ready then
            mesh.dbus_set_property(watcher.local_service, "RegisteredStatusNotifierItems", registered())
            mesh.dbus_set_property(watcher.local_service, "IsStatusNotifierHostRegistered", host_ready or next(hosts) ~= nil)
        end
    end
end

local function emit(member, id)
    for _, watcher in pairs(watchers) do
        if watcher.local_service and watcher.ready then mesh.dbus_emit(watcher.local_service, member, { id }) end
    end
end

local function remove(id)
    local entry = entries[id]
    if not entry then return end
    for _, handle in { entry.subscription, entry.property_subscription } do
        if handle then subscriptions[handle.id] = nil; mesh.close_stream(handle) end
    end
    entries[id] = nil
    emit("StatusNotifierItemUnregistered", id)
    publish()
end

local function text(value, limit)
    return type(value) == "string" and value:sub(1, limit or 128):gsub("[%c]", " ") or ""
end

-- The pixmap closest to 32px becomes a blob; the shell scales it.
local function pixmap(value)
    if type(value) ~= "table" then return nil end
    local best, best_size
    for _, candidate in value do
        local fields = plain(candidate)
        if type(fields) ~= "table" then continue end
        local width, height, bytes = plain(fields[1]), plain(fields[2]), plain(fields[3])
        if type(width) == "number" and type(height) == "number" and width > 0 and height > 0
            and width <= 512 and height <= 512 and type(bytes) == "table" and #bytes == width * height * 4 then
            local distance = math.abs(math.max(width, height) - 32)
            if not best_size or distance < best_size then best, best_size = { width, height, bytes }, distance end
        end
    end
    if not best then return nil end
    local ok, handle = pcall(mesh.blob.put, best[3], "image/x-mesh-argb32", { width = best[1], height = best[2] })
    return ok and handle or nil
end

local PROPERTY_NAMES = { "Title", "Id", "Category", "Status", "IconName", "AttentionIconName", "OverlayIconName", "IconThemePath", "ToolTip", "ItemIsMenu", "Menu", "IconPixmap", "AttentionIconPixmap" }
local function item_properties(entry, interface)
    local props = properties(entry.owner, entry.path, interface)
    if props then return props end
    -- Large optional pixmaps must not hide an otherwise valid named item.
    props = {}
    for _, name in PROPERTY_NAMES do
        local reply = call(entry.owner, entry.path, "org.freedesktop.DBus.Properties", "Get", "ss", { interface, name })
        if reply.success then props[name] = plain(reply.body[1]) end
    end
    return next(props) and props or nil
end

local function refresh(entry)
    local props
    for _, interface in ITEMS do
        props = item_properties(entry, interface)
        if props and type(props.Id) == "string" and type(props.Status) == "string" then entry.interface = interface; break end
        props = nil
    end
    if not props then return false end
    local tooltip = type(props.ToolTip) == "table" and props.ToolTip or {}
    entry.item = {
        id = entry.id, title = text(props.Title), application_id = text(props.Id),
        category = text(props.Category), status = text(props.Status),
        icon_name = text(props.IconName), attention_icon_name = text(props.AttentionIconName),
        overlay_icon_name = text(props.OverlayIconName), icon_theme_path = text(props.IconThemePath),
        tooltip_title = text(plain(tooltip[3])), tooltip_text = text(plain(tooltip[4])),
        item_is_menu = props.ItemIsMenu == true, menu = text(props.Menu),
    }
    if entry.item.status == "NeedsAttention" and entry.item.attention_icon_name ~= "" then
        entry.item.icon_name = entry.item.attention_icon_name
    end
    local pixels = entry.item.status == "NeedsAttention" and props.AttentionIconPixmap or props.IconPixmap
    if entry.item.icon_name == "" then entry.item.icon_pixmap = pixmap(pixels) end
    if not entry.subscription then
        entry.subscription = mesh.dbus_subscribe("session", {
            sender = entry.owner, path_namespace = entry.path, interface = entry.interface,
        })
        if entry.subscription then subscriptions[entry.subscription.id] = entry.id end
    end
    if not entry.property_subscription then
        entry.property_subscription = mesh.dbus_subscribe("session", {
            sender = entry.owner, path_namespace = entry.path, interface = "org.freedesktop.DBus.Properties",
        })
        if entry.property_subscription then subscriptions[entry.property_subscription.id] = entry.id end
    end
    return true
end

local function add(service, sender, source)
    if type(service) ~= "string" or #service > 256 then return nil end
    local destination, path
    if service:sub(1, 1) == "/" then
        destination, path = sender, service
    else
        local split = service:find("/", 1, true)
        destination = split and service:sub(1, split - 1) or service
        path = split and service:sub(split) or "/StatusNotifierItem"
    end
    if not destination or not path then return nil end
    local unique = owner(destination)
    if not unique then return nil end
    local id = unique .. path
    if entries[id] then entries[id].sources[source] = true; return id end
    if #registered() >= MAX_ITEMS then return nil end
    local entry = { id = id, owner = unique, service = destination, path = path, sources = { [source] = true } }
    if not refresh(entry) then return nil end
    entries[id] = entry
    emit("StatusNotifierItemRegistered", id)
    publish()
    return id
end

local function reconcile(watcher)
    local props = properties(watcher.name, PATH, watcher.name)
    if not props then return false end
    local live = {}
    for _, service in props.RegisteredStatusNotifierItems or {} do
        local id = add(service, nil, watcher.name)
        if id then live[id] = true end
    end
    local gone = {}
    for id, entry in pairs(entries) do
        if entry.sources[watcher.name] and not live[id] then
            entry.sources[watcher.name] = nil
            if next(entry.sources) == nil then gone[#gone + 1] = id end
        end
    end
    for _, id in gone do remove(id) end
    local current_owner = owner(watcher.name)
    if current_owner and watcher.registered_owner ~= current_owner then
        local reply = call(watcher.name, PATH, watcher.name, "RegisterStatusNotifierHost", "s", { HOST })
        if reply.success then watcher.registered_owner = current_owner end
    end
    watcher.remote = true
    return true
end

local function subscribe_watcher(watcher)
    if not watcher.subscription then
        watcher.subscription = mesh.dbus_subscribe("session", {
            sender = watcher.name, path_namespace = PATH, interface = watcher.name,
        })
    end
end

local function connect(watcher)
    if watcher.local_service or reconcile(watcher) then return end
    watcher.local_service = mesh.dbus_serve("session", {
        name = watcher.name, path = PATH, interface = watcher.name,
        methods = {
            RegisterStatusNotifierItem = { ["in"] = "s" },
            RegisterStatusNotifierHost = { ["in"] = "s" },
        },
        signals = { StatusNotifierItemRegistered = "s", StatusNotifierItemUnregistered = "s", StatusNotifierHostRegistered = "" },
        properties = {
            RegisteredStatusNotifierItems = { signature = "as", value = registered() },
            IsStatusNotifierHostRegistered = { signature = "b", value = host_ready },
            ProtocolVersion = { signature = "i", value = 0 },
        },
    })
end

local function connect_host()
    if not host then host = mesh.dbus_serve("session", { name = HOST, path = "/StatusNotifierHost", interface = "org.kde.StatusNotifierHost" }) end
end

function start(self)
    connect_host()
    names = mesh.dbus_subscribe("session", {
        sender = "org.freedesktop.DBus", path_namespace = "/org/freedesktop/DBus",
        interface = "org.freedesktop.DBus", member = "NameOwnerChanged",
    })
    for _, name in WATCHERS do
        local watcher = { name = name }
        watchers[name] = watcher
        subscribe_watcher(watcher)
    end
    mesh.service.set_poll_interval(10000)
end

function on_poll(self)
    connect_host()
    for _, watcher in pairs(watchers) do
        subscribe_watcher(watcher)
        if not watcher.local_service then connect(watcher) end
    end
    local gone = {}
    for id, entry in pairs(entries) do
        if not owner(entry.owner) then gone[#gone + 1] = id else refresh(entry) end
    end
    for _, id in gone do remove(id) end
    publish()
end

function on_stream_event(self, event)
    if host and event.stream.id == host.id then
        if event.type == "started" then
            host_ready = true
            for _, watcher in pairs(watchers) do connect(watcher) end
            publish()
        elseif event.type == "failed" or event.type == "exited" then
            host, host_ready = nil, false
            publish()
        end
        return
    end
    if event.type ~= "line" then
        local id = subscriptions[event.stream.id]
        if id and entries[id] then
            local entry = entries[id]
            if event.type == "failed" or event.type == "exited" then
                if entry.subscription and entry.subscription.id == event.stream.id then entry.subscription = nil end
                if entry.property_subscription and entry.property_subscription.id == event.stream.id then entry.property_subscription = nil end
                subscriptions[event.stream.id] = nil
            elseif event.type == "overflow" then refresh(entry); publish() end
            return
        end
        for _, watcher in pairs(watchers) do
            if watcher.subscription and event.stream.id == watcher.subscription.id and (event.type == "failed" or event.type == "exited") then watcher.subscription = nil end
            if watcher.local_service and event.stream.id == watcher.local_service.id then
                if event.type == "started" then
                    watcher.ready = true
                    mesh.dbus_emit(watcher.local_service, "StatusNotifierHostRegistered", {})
                    publish()
                elseif event.type == "failed" or event.type == "exited" then
                    watcher.local_service, watcher.ready = nil, false
                    reconcile(watcher)
                end
            end
        end
        return
    end
    local ok, record = pcall(mesh.json.decode, event.line)
    if not ok then return end
    if names and event.stream.id == names.id then
        local name, old, new = plain(record.body[1]), plain(record.body[2]), plain(record.body[3])
        local host_changed = hosts[name] ~= nil
        if old ~= "" and new ~= old then
            hosts[name] = nil
            if watchers[name] then watchers[name].registered_owner = nil end
            local gone = {}
            for id, entry in pairs(entries) do
                if entry.owner == name or entry.service == name then gone[#gone + 1] = id end
            end
            for _, id in gone do remove(id) end
        end
        if watchers[name] and new ~= "" and not watchers[name].local_service then
            reconcile(watchers[name])
            publish()
        elseif host_changed then publish() end
        return
    end
    local id = subscriptions[event.stream.id]
    if id and entries[id] then refresh(entries[id]); publish(); return end
    for _, watcher in pairs(watchers) do
        if watcher.local_service and event.stream.id == watcher.local_service.id and record.type == "call" then
            local service = plain((record.body or {})[1])
            local valid
            if record.member == "RegisterStatusNotifierItem" then
                valid = add(service, record.sender, watcher.name)
            elseif record.member == "RegisterStatusNotifierHost" then
                valid = type(service) == "string" and owner(service)
                if valid and not hosts[valid] then
                    local count = 0
                    for _ in pairs(hosts) do count += 1 end
                    if count >= MAX_ITEMS then valid = nil end
                end
                if valid and not hosts[valid] then
                    hosts[valid] = true
                    mesh.dbus_emit(watcher.local_service, "StatusNotifierHostRegistered", {})
                    publish()
                end
            end
            if valid then mesh.dbus_reply(watcher.local_service, record.id, {})
            else mesh.dbus_reply_error(watcher.local_service, record.id, "org.freedesktop.DBus.Error.InvalidArgs", "invalid or unavailable StatusNotifier service") end
            return
        elseif watcher.subscription and event.stream.id == watcher.subscription.id then
            if not watcher.local_service and (record.member == "StatusNotifierItemRegistered" or record.member == "StatusNotifierItemUnregistered") then reconcile(watcher); publish() end
            return
        end
    end
end

local function invoke(id, member, signature, args)
    local entry = entries[id]
    if not entry then return { ok = false, error = "tray item is no longer registered" } end
    local reply = call(entry.owner, entry.path, entry.interface, member, signature, args)
    return { ok = reply.success, error = reply.error or "" }
end

function on_command_activate(self)
    local args = mesh.service.payload()
    local entry = entries[args.id]
    local member = entry and entry.item and entry.item.item_is_menu and "ContextMenu" or "Activate"
    return invoke(args.id, member, "ii", { args.x, args.y })
end
function on_command_secondary_activate(self)
    local args = mesh.service.payload()
    return invoke(args.id, "SecondaryActivate", "ii", { args.x, args.y })
end
function on_command_context_menu(self)
    local args = mesh.service.payload()
    return invoke(args.id, "ContextMenu", "ii", { args.x, args.y })
end
function on_command_scroll(self)
    local args = mesh.service.payload()
    if args.orientation ~= "horizontal" and args.orientation ~= "vertical" then return { ok = false, error = "invalid orientation" } end
    return invoke(args.id, "Scroll", "is", { args.delta, args.orientation })
end
