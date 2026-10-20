-- Linux system load provider.
--
-- Processor and network figures are rates, so each poll compares counters
-- with the previous sample. The disk only changes slowly and costs a process,
-- so it is re-read every DISK_EVERY polls.

local POLL_MS = 2000
local HISTORY = 30
local DISK_EVERY = 15
-- Processor package sensors, most specific first.
local CPU_SENSORS = { "k10temp", "zenpower", "coretemp", "cpu_thermal", "acpitz" }

state = {
    available = false,
    cpu = 0,
    cpu_history = {},
    memory_used = 0,
    memory_total = 0,
    disk_used = 0,
    disk_total = 0,
    temperature = nil,
    net_rx = 0,
    net_tx = 0,
    uptime = 0,
}

local previous_cpu = nil
local previous_net = nil
local history = {}
local polls = 0
local disk = { used = 0, total = 0 }
local sensor = nil

local function read(path)
    local reply = mesh.read_file(path)
    return reply.success and reply.contents or nil
end

local function cpu_counters()
    local text = read("/proc/stat")
    local line = text and text:match("^cpu%s+([^\n]+)")
    if not line then
        return nil
    end
    local total, idle, index = 0, 0, 0
    for value in line:gmatch("%d+") do
        index += 1
        local number = tonumber(value) or 0
        -- guest and guest_nice are already counted in user and nice.
        if index <= 8 then
            total += number
        end
        if index == 4 or index == 5 then
            idle += number
        end
    end
    return { total = total, idle = idle }
end

local function memory()
    local text = read("/proc/meminfo") or ""
    local total = tonumber(text:match("MemTotal:%s+(%d+)")) or 0
    local available = tonumber(text:match("MemAvailable:%s+(%d+)")) or 0
    return (total - available) * 1024, total * 1024
end

local function network_counters()
    local text = read("/proc/net/dev")
    if not text then
        return nil
    end
    local rx, tx = 0, 0
    for name, fields in text:gmatch("\n%s*([^:%s]+):([^\n]+)") do
        if name ~= "lo" then
            local values = {}
            for value in fields:gmatch("%d+") do
                values[#values + 1] = tonumber(value)
            end
            rx += values[1] or 0
            tx += values[9] or 0
        end
    end
    return { rx = rx, tx = tx }
end

local function find_sensor()
    local found = {}
    for index = 0, 15 do
        local name = read("/sys/class/hwmon/hwmon" .. index .. "/name")
        if name then
            found[name:match("^%s*(.-)%s*$")] = "/sys/class/hwmon/hwmon" .. index .. "/temp1_input"
        end
    end
    for _, candidate in CPU_SENSORS do
        if found[candidate] then
            return found[candidate]
        end
    end
    return nil
end

local function temperature()
    local text = sensor and read(sensor)
    local millidegrees = text and tonumber(text:match("%-?%d+"))
    return millidegrees and millidegrees / 1000 or nil
end

local function read_disk()
    local result = mesh.exec("df", { "-B1", "--output=size,used", "/" })
    local total, used = (result.stdout or ""):match("\n%s*(%d+)%s+(%d+)")
    if total then
        disk = { total = tonumber(total), used = tonumber(used) }
    end
end

local function sample()
    local cpu = state.cpu
    local counters = cpu_counters()
    if counters and previous_cpu then
        local total = counters.total - previous_cpu.total
        local idle = counters.idle - previous_cpu.idle
        if total > 0 then
            cpu = math.clamp((1 - idle / total) * 100, 0, 100)
        end
    end
    previous_cpu = counters or previous_cpu
    if counters then
        history[#history + 1] = cpu
        while #history > HISTORY do
            table.remove(history, 1)
        end
    end

    local rx_rate, tx_rate = state.net_rx, state.net_tx
    local net = network_counters()
    if net and previous_net then
        local elapsed = POLL_MS / 1000
        rx_rate = math.max(0, (net.rx - previous_net.rx) / elapsed)
        tx_rate = math.max(0, (net.tx - previous_net.tx) / elapsed)
    end
    previous_net = net or previous_net

    if polls % DISK_EVERY == 0 then
        read_disk()
    end
    polls += 1

    local used, total = memory()
    local uptime = tonumber((read("/proc/uptime") or ""):match("^([%d%.]+)")) or 0
    state = {
        available = counters ~= nil,
        cpu = cpu,
        cpu_history = table.clone(history),
        memory_used = used,
        memory_total = total,
        disk_used = disk.used,
        disk_total = disk.total,
        temperature = temperature(),
        net_rx = rx_rate,
        net_tx = tx_rate,
        uptime = uptime,
    }
end

function start(self)
    sensor = find_sensor()
    sample()
    mesh.service.set_poll_interval(POLL_MS)
end

function on_poll(self)
    sample()
end
