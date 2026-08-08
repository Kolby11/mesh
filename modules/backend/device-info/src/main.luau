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
    return (value or ""):match("^%s*(.-)%s*$")
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

local function unquote(value)
    value = trim(value)
    if value:sub(1, 1) == '"' and value:sub(-1) == '"' then
        value = value:sub(2, -2)
        value = value:gsub('\\"', '"'):gsub('\\\\', '\\')
    end
    return value
end

local function parse_os_release(content)
    local values = {}
    for line in (content or ""):gmatch("([^\n]+)") do
        local key, value = line:match("^([%w_]+)=(.*)$")
        if key and value then
            values[key] = unquote(value)
        end
    end
    return values
end

local function parse_processor(content)
    local processor = (content or ""):match("model name%s*:%s*([^\n]+)")
    if processor then return trim(processor) end

    processor = (content or ""):match("Hardware%s*:%s*([^\n]+)")
    if processor then return trim(processor) end

    processor = (content or ""):match("Processor%s*:%s*([^\n]+)")
    return trim(processor)
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
    local release = parse_os_release(run("cat", { "/etc/os-release" }))
    local hostname = run("hostname", {})
    local desktop = run("printenv", { "XDG_CURRENT_DESKTOP" })
    if desktop == "" then
        desktop = run("printenv", { "DESKTOP_SESSION" })
    end

    local os_name = first_non_empty(release.NAME, release.PRETTY_NAME, system)
    local os_version = first_non_empty(release.VERSION_ID, release.VERSION)
    local processor = parse_processor(run("cat", { "/proc/cpuinfo" }))
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
