-- Linux device-information backend.
--
-- This provider deliberately reports facts through generic host execution APIs.
-- Settings owns presentation; the backend only normalizes the host's system
-- identity, hardware summary, and runtime uptime into mesh.device state.

state = {
    available = false,
    hostname = "",
    os_name = "",
    os_version = "",
    kernel = "",
    architecture = "",
    desktop = "",
    processor = "",
    memory_total_bytes = 0,
    graphics = "",
    uptime_seconds = 0,
}

local function trim(value)
    value = value or ""
    local first = 1
    local last = #value
    while first <= last do
        local byte = value:byte(first)
        if byte ~= 9 and byte ~= 10 and byte ~= 11 and byte ~= 12 and byte ~= 13 and byte ~= 32 then
            break
        end
        first = first + 1
    end
    while last >= first do
        local byte = value:byte(last)
        if byte ~= 9 and byte ~= 10 and byte ~= 11 and byte ~= 12 and byte ~= 13 and byte ~= 32 then
            break
        end
        last = last - 1
    end
    return value:sub(first, last)
end

local function first_non_empty(...)
    for _, value in ipairs({ ... }) do
        value = trim(value)
        if value ~= "" then return value end
    end
    return ""
end

local function run(program, args)
    local result = mesh.exec(program, args or {})
    if not result.success then
        return ""
    end
    return trim(result.stdout)
end

local function run_raw(program, args)
    local result = mesh.exec(program, args or {})
    if not result.success then
        return ""
    end
    return result.stdout or ""
end

local function unquote(value)
    value = trim(value)
    if value:sub(1, 1) == '"' and value:sub(-1) == '"' then
        value = value:sub(2, -2)
        value = value:gsub('\\"', '"'):gsub('\\\\', '\\')
    end
    return value
end

local function parse_os_release(content)
    content = content or ""
    local values = {}
    for _, key in ipairs({ "NAME", "PRETTY_NAME", "VERSION_ID", "VERSION" }) do
        local start = content:find("\n" .. key .. "=", 1, true)
        if start then
            start = start + 1
        elseif content:sub(1, #key + 1) == key .. "=" then
            start = 1
        end
        if start then
            local value_start = start + #key + 1
            local line_end = content:find("\n", value_start, true) or (#content + 1)
            values[key] = unquote(content:sub(value_start, line_end - 1))
        end
    end
    return values
end

local function parse_processor(content)
    content = content or ""
    for _, key in ipairs({ "model name", "Hardware", "Processor" }) do
        local start = content:find(key, 1, true)
        if start then
            local line_end = content:find("\n", start, true) or (#content + 1)
            local line = content:sub(start, line_end - 1)
            local colon = line:find(":", 1, true)
            if colon then
                return trim(line:sub(colon + 1))
            end
        end
    end
    return ""
end

local function parse_memory_bytes(content)
    local kilobytes = tonumber((content or ""):match("MemTotal:%s*(%d+)%s*kB"))
    if not kilobytes then return 0 end
    return kilobytes * 1024
end

local function parse_uptime_seconds(content)
    local seconds = tonumber((content or ""):match("^([%d%.]+)"))
    if not seconds then return 0 end
    return math.floor(seconds)
end

local function parse_graphics()
    if not mesh.service.can_exec("lspci", {}) then
        return ""
    end
    local output = run("lspci", {})
    if output == "" then return "" end

    for line in output:gmatch("([^\n]+)") do
        local lower = line:lower()
        if lower:find("vga compatible controller", 1, true)
            or lower:find("3d controller", 1, true)
            or lower:find("display controller", 1, true)
        then
            local description = line:match("controller:%s*(.+)$")
            return trim(description or line)
        end
    end
    return ""
end

local function read_state()
    local kernel = run("uname", { "-r" })
    local architecture = run("uname", { "-m" })
    local system = run("uname", { "-s" })
    local os_release = run("cat", { "/etc/os-release" })
    local release = parse_os_release(os_release)
    local hostname = run("hostname", {})
    local desktop = ""
    if mesh.service.can_exec("printenv", { "XDG_CURRENT_DESKTOP" }) then
        desktop = run("printenv", { "XDG_CURRENT_DESKTOP" })
    end
    if desktop == "" then
        if mesh.service.can_exec("printenv", { "DESKTOP_SESSION" }) then
            desktop = run("printenv", { "DESKTOP_SESSION" })
        end
    end

    local os_name = first_non_empty(release.NAME, release.PRETTY_NAME, system)
    local os_version = first_non_empty(release.VERSION_ID, release.VERSION)
    local cpuinfo = run_raw("cat", { "/proc/cpuinfo" })
    local processor = parse_processor(cpuinfo)
    local memory_total_bytes = parse_memory_bytes(run("cat", { "/proc/meminfo" }))

    return {
        available = kernel ~= "" or hostname ~= "",
        hostname = hostname,
        os_name = os_name or "",
        os_version = os_version,
        kernel = kernel,
        architecture = architecture,
        desktop = desktop,
        processor = processor,
        memory_total_bytes = memory_total_bytes,
        graphics = parse_graphics(),
        uptime_seconds = parse_uptime_seconds(run("cat", { "/proc/uptime" })),
    }
end

function start(self)
    -- Identity and hardware change rarely; uptime is refreshed often enough
    -- for an open settings page without repeatedly spawning expensive probes.
    mesh.service.set_poll_interval(30000)
    state = read_state()
end

function on_poll(self)
    state = read_state()
end
