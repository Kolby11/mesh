-- Machine identity, processor, memory, graphics and storage.
--
-- Identity and processor facts are read once; memory and disks are re-read on a
-- slow poll.

state = {
    available = false,
    vendor = "", model = "", product_version = "", bios_version = "",
    cpu_model = "", cpu_cores = 0, cpu_threads = 0, cpu_max_mhz = 0,
    memory_total_bytes = 0, memory_available_bytes = 0, swap_total_bytes = 0, swap_free_bytes = 0,
    gpus = {}, disks = {},
}

local POLL_MS = 30000
local identity = nil

local function trim(value)
    return ((value or ""):gsub("^%s+", ""):gsub("%s+$", ""))
end

local function cat(path)
    local result = mesh.exec("cat", { path })
    if not result.success then return "" end
    return result.stdout or ""
end

local function read_identity()
    local cpuinfo = cat("/proc/cpuinfo")
    local threads = 0
    for _ in cpuinfo:gmatch("\nprocessor%s*:") do threads = threads + 1 end
    if cpuinfo:match("^processor%s*:") then threads = threads + 1 end
    local max_khz = tonumber(trim(cat("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"))) or 0
    local gpus = {}
    local listing = mesh.exec("lspci", {})
    if listing.success then
        for line in (listing.stdout or ""):gmatch("[^\n]+") do
            local kind, name = line:match("^%S+%s+([^:]+):%s*(.*)$")
            if kind and (kind:find("VGA", 1, true) or kind:find("3D", 1, true) or kind:find("Display", 1, true)) then
                gpus[#gpus + 1] = trim((name:gsub("%s*%(rev [%x]+%)", "")))
            end
        end
    end
    identity = {
        vendor = trim(cat("/sys/class/dmi/id/sys_vendor")),
        model = trim(cat("/sys/class/dmi/id/product_name")),
        product_version = trim(cat("/sys/class/dmi/id/product_version")),
        bios_version = trim(cat("/sys/class/dmi/id/bios_version")),
        cpu_model = trim(cpuinfo:match("model name%s*:%s*([^\n]+)") or ""),
        cpu_cores = tonumber(cpuinfo:match("cpu cores%s*:%s*(%d+)")) or threads,
        cpu_threads = threads,
        cpu_max_mhz = math.floor(max_khz / 1000),
        gpus = gpus,
    }
end

local function kib(meminfo, key)
    return (tonumber(meminfo:match(key .. ":%s*(%d+) kB")) or 0) * 1024
end

local function read_disks()
    local disks = {}
    local result = mesh.exec("df", { "-B1", "--output=source,fstype,size,used,target", "-x", "tmpfs", "-x", "devtmpfs", "-x", "squashfs", "-x", "overlay", "-x", "efivarfs", "-x", "ramfs" })
    if not result.success then return disks end
    local first = true
    for line in (result.stdout or ""):gmatch("[^\n]+") do
        if first then
            first = false
        else
            local device, fs, size, used, mount = line:match("^(%S+)%s+(%S+)%s+(%d+)%s+(%d+)%s+(.+)$")
            if device and device:sub(1, 5) == "/dev/" then
                disks[#disks + 1] = { device = device, mount = mount, filesystem = fs, total_bytes = tonumber(size) or 0, used_bytes = tonumber(used) or 0 }
            end
        end
    end
    return disks
end

local function refresh()
    if not identity then read_identity() end
    local meminfo = cat("/proc/meminfo")
    state = {
        available = true,
        vendor = identity.vendor, model = identity.model, product_version = identity.product_version, bios_version = identity.bios_version,
        cpu_model = identity.cpu_model, cpu_cores = identity.cpu_cores, cpu_threads = identity.cpu_threads, cpu_max_mhz = identity.cpu_max_mhz,
        memory_total_bytes = kib(meminfo, "MemTotal"), memory_available_bytes = kib(meminfo, "MemAvailable"),
        swap_total_bytes = kib(meminfo, "SwapTotal"), swap_free_bytes = kib(meminfo, "SwapFree"),
        gpus = identity.gpus, disks = read_disks(),
    }
end

function start(self)
    mesh.service.set_poll_interval(POLL_MS)
    refresh()
end

function on_poll(self)
    refresh()
end

function on_command_refresh(self)
    refresh()
    return { ok = true }
end
