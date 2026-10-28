-- Desktop notification daemon (freedesktop Desktop Notifications 1.2).
--
-- The host owns the bus name and validates call signatures; this provider
-- answers Notify/CloseNotification/GetCapabilities/GetServerInformation and
-- keeps the open list. A popup that times out stays listed until dismissed,
-- like a notification centre; NotificationClosed is sent when it leaves the list.

local SERVICE = {
    name = "org.freedesktop.Notifications",
    path = "/org/freedesktop/Notifications",
    interface = "org.freedesktop.Notifications",
    methods = {
        Notify = { ["in"] = "susssasa{sv}i", out = "u" },
        CloseNotification = { ["in"] = "u" },
        GetCapabilities = { out = "as" },
        GetServerInformation = { out = "ssss" },
    },
    signals = {
        NotificationClosed = "uu",
        ActionInvoked = "us",
    },
}

-- NotificationClosed reasons.
local EXPIRED, DISMISSED, CLOSED_BY_CALL = 1, 2, 3

local SWEEP_MS = 500

state = { available = false, notifications = {}, dnd = false }

local service = nil
local next_id = 1
-- id -> unix time in ms at which the popup hides; nil for no deadline.
local popup_deadline = {}

local function setting(name, fallback)
    local ok, config = pcall(mesh.config)
    local value = ok and type(config) == "table" and config[name]
    if value == nil then
        return fallback
    end
    return value
end

local function now_ms()
    return os.time() * 1000
end

-- Body fields arrive as {signature, value} records; nested variants too.
local function plain(field)
    if type(field) == "table" and field.signature ~= nil and field.value ~= nil then
        return plain(field.value)
    end
    return field
end

local ENTITIES = { amp = "&", lt = "<", gt = ">", quot = "\"", apos = "'" }

local function strip_markup(text)
    text = tostring(text or ""):gsub("<[^>]*>", "")
    return (text:gsub("&(%a+);", function(name)
        return ENTITIES[name] or ("&" .. name .. ";")
    end))
end

local function local_path(value)
    value = tostring(value or "")
    local path = value:match("^file://(.+)$") or value
    return path:sub(1, 1) == "/" and path or ""
end

-- The image-data hint is (iiibiiay): width, height, rowstride, has_alpha,
-- bits_per_sample, channels, data. Older senders use image_data or icon_data.
local function image_data(hints)
    local raw = plain(hints["image-data"] or hints.image_data or hints.icon_data)
    if type(raw) ~= "table" then
        return nil
    end
    local width, height, stride = plain(raw[1]), plain(raw[2]), plain(raw[3])
    local alpha, bits, channels, bytes = plain(raw[4]), plain(raw[5]), plain(raw[6]), plain(raw[7])
    if bits ~= 8 or type(bytes) ~= "table" then
        return nil
    end
    local mime = if alpha and channels == 4 then "image/x-mesh-rgba8"
        elseif not alpha and channels == 3 then "image/x-mesh-rgb8"
        else nil
    if not mime then
        return nil
    end
    local ok, handle = pcall(mesh.blob.put, bytes, mime, { width = width, height = height, stride = stride })
    return ok and handle or nil
end

local function find(id)
    for index, notification in state.notifications do
        if notification.id == id then
            return index, notification
        end
    end
    return nil, nil
end

local function publish(list)
    state = { available = state.available, notifications = list, dnd = state.dnd }
end

local function without(id)
    local list = {}
    for _, notification in state.notifications do
        if notification.id ~= id then
            list[#list + 1] = notification
        end
    end
    return list
end

local function close(id, reason)
    if not find(id) then
        return false
    end
    popup_deadline[id] = nil
    publish(without(id))
    if service then
        mesh.dbus_emit(service, "NotificationClosed", { id, reason })
    end
    return true
end

local function notify(args)
    local app_name = plain(args[1]) or ""
    local replaces = plain(args[2]) or 0
    local app_icon = plain(args[3]) or ""
    local summary = plain(args[4]) or ""
    local body = plain(args[5]) or ""
    local raw_actions = plain(args[6]) or {}
    local hints = plain(args[7]) or {}
    local timeout = plain(args[8]) or -1

    local id = replaces
    if id == 0 or not find(id) then
        id = next_id
        next_id += 1
    end

    local actions = {}
    for index = 1, #raw_actions - 1, 2 do
        actions[#actions + 1] = { key = tostring(raw_actions[index]), label = tostring(raw_actions[index + 1]) }
    end

    local urgency = tonumber(plain(hints.urgency)) or 1
    local image = local_path(plain(hints["image-path"]) or plain(hints.image_path) or "")
    local icon = local_path(app_icon)
    if icon == "" then
        icon = app_icon
    end

    local popup = not state.dnd or urgency >= 2
    local notification = {
        id = id,
        app_name = tostring(app_name),
        app_icon = tostring(icon),
        image = image,
        image_data = image_data(hints),
        summary = strip_markup(summary),
        body = strip_markup(body),
        urgency = urgency,
        actions = actions,
        time = os.time(),
        popup = popup,
    }

    local list = { notification }
    for _, existing in without(id) do
        list[#list + 1] = existing
    end
    local limit = math.max(1, math.floor(tonumber(setting("history", 50)) or 50))
    for index = #list, limit + 1, -1 do
        popup_deadline[list[index].id] = nil
        if service then
            mesh.dbus_emit(service, "NotificationClosed", { list[index].id, EXPIRED })
        end
        table.remove(list, index)
    end
    publish(list)

    if popup then
        if timeout == 0 or (timeout < 0 and urgency >= 2) then
            popup_deadline[id] = nil
        else
            local seconds = tonumber(setting("timeout_seconds", 6)) or 6
            local ms = timeout > 0 and timeout or seconds * 1000
            popup_deadline[id] = now_ms() + ms
        end
    end
    return id
end

local function set_popup(id, popup)
    local index, notification = find(id)
    if not index or notification.popup == popup then
        return
    end
    local list = table.clone(state.notifications)
    local updated = table.clone(notification)
    updated.popup = popup
    list[index] = updated
    publish(list)
end

local function handle_call(call)
    local args = call.body or {}
    if call.member == "Notify" then
        mesh.dbus_reply(service, call.id, { notify(args) })
    elseif call.member == "CloseNotification" then
        close(plain(args[1]) or 0, CLOSED_BY_CALL)
        mesh.dbus_reply(service, call.id, {})
    elseif call.member == "GetCapabilities" then
        mesh.dbus_reply(service, call.id, { { "body", "actions", "icon-static", "persistence" } })
    elseif call.member == "GetServerInformation" then
        mesh.dbus_reply(service, call.id, { "MESH", "MESH", "0.1.0", "1.2" })
    else
        mesh.dbus_reply_error(service, call.id, "org.freedesktop.DBus.Error.UnknownMethod", call.member)
    end
end

function start(self)
    service = mesh.dbus_serve("session", {
        name = SERVICE.name,
        path = SERVICE.path,
        interface = SERVICE.interface,
        methods = SERVICE.methods,
        signals = SERVICE.signals,
        replace = setting("replace_daemon", false) == true,
    })
    if not service then
        mesh.log.warn("org.freedesktop.Notifications could not be served")
        return
    end
    mesh.service.set_poll_interval(SWEEP_MS)
end

function on_stream_event(self, event)
    if not service or event.stream.id ~= service.id then
        return
    end
    if event.type == "started" then
        state = { available = true, notifications = state.notifications, dnd = state.dnd }
    elseif event.type == "line" then
        local ok, call = pcall(mesh.json.decode, event.line)
        if ok and type(call) == "table" and call.type == "call" then
            handle_call(call)
        end
    elseif event.type == "failed" then
        mesh.log.warn("notification daemon stopped: " .. tostring(event.error or ""))
    elseif event.type == "exited" then
        service = nil
        state = { available = false, notifications = state.notifications, dnd = state.dnd }
        mesh.service.set_poll_interval(0)
    end
end

function on_poll(self)
    local current = now_ms()
    for id, deadline in popup_deadline do
        if current >= deadline then
            popup_deadline[id] = nil
            set_popup(id, false)
        end
    end
end

function on_command_dismiss(self)
    local id = tonumber(mesh.service.payload().id) or 0
    if not close(id, DISMISSED) then
        return { ok = false, error = "no notification " .. id }
    end
    return { ok = true }
end

function on_command_hide_popup(self)
    local id = tonumber(mesh.service.payload().id) or 0
    popup_deadline[id] = nil
    set_popup(id, false)
    return { ok = true }
end

function on_command_invoke(self)
    local payload = mesh.service.payload()
    local id = tonumber(payload.id) or 0
    local action = tostring(payload.action or "default")
    if not find(id) then
        return { ok = false, error = "no notification " .. id }
    end
    if service then
        mesh.dbus_emit(service, "ActionInvoked", { id, action })
    end
    close(id, DISMISSED)
    return { ok = true }
end

function on_command_clear(self)
    for _, notification in table.clone(state.notifications) do
        close(notification.id, DISMISSED)
    end
    return { ok = true }
end

function on_command_set_dnd(self)
    local enabled = mesh.service.payload().enabled == true
    state = { available = state.available, notifications = state.notifications, dnd = enabled }
    return { ok = true }
end
