-- Audio spectrum provider over cava's raw ASCII output.
--
-- cava sleeps after one second of silence and stops writing frames, so the
-- provider marks itself inactive on an all-zero frame and publishes nothing
-- until sound returns.

local CAVA = "c=\"${XDG_RUNTIME_DIR:-/tmp}/mesh-cava.conf\"; printf '[general]\\nbars = 24\\nframerate = 30\\nsleep_timer = 1\\n[output]\\nmethod = raw\\nraw_target = /dev/stdout\\ndata_format = ascii\\nascii_max_range = 1000\\nbar_delimiter = 59\\nframe_delimiter = 10\\n[smoothing]\\nnoise_reduction = 77\\n' > \"$c\" && exec cava -p \"$c\""
local BARS = 24
local RESTART_MS = 5000

state = { available = false, active = false, bars = {} }

local source = nil

local function silent_bars()
    local bars = {}
    for index = 1, BARS do
        bars[index] = 0
    end
    return bars
end

local function launch()
    source = mesh.exec_stream("sh", { "-c", CAVA })
    if not source then
        state = { available = false, active = false, bars = silent_bars() }
        return false
    end
    return true
end

local function publish(line)
    local bars, sum = {}, 0
    for value in line:gmatch("%d+") do
        local level = math.min(1, (tonumber(value) or 0) / 1000)
        bars[#bars + 1] = level
        sum += level
    end
    if #bars == 0 then
        return
    end
    local active = sum > 0
    if not active and not state.active and state.available then
        return
    end
    state = { available = true, active = active, bars = bars }
end

function start(self)
    state = { available = false, active = false, bars = silent_bars() }
    mesh.service.set_poll_interval(launch() and 0 or RESTART_MS)
end

function on_stream_batch(self, program, lines)
    if program == "sh" and source and #lines > 0 then
        publish(lines[#lines])
    end
end

function on_stream_closed(self, program, _success)
    if program ~= "sh" then return end
    source = nil
    state = { available = false, active = false, bars = silent_bars() }
    mesh.service.set_poll_interval(RESTART_MS)
end

function on_poll(self)
    if not source and launch() then
        mesh.service.set_poll_interval(0)
    end
end
