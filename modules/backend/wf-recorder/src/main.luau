-- wf-recorder screen recording provider.
--
-- A recording is a wf-recorder exec stream; SIGINT makes it finalize the file,
-- and the stream closing is the single place the provider returns to idle. A
-- slow poll notices a recorder started outside MESH.

local POLL_MS = 10000

state = { available = true, recording = false, started_at = 0, output = "" }

local owned = false

local function videos_dir()
    local result = mesh.exec("sh", { "-c", "d=\"${XDG_VIDEOS_DIR:-$HOME/Videos}/Recordings\"; mkdir -p \"$d\" && printf '%s' \"$d\"" })
    return result.success and (result.stdout or "") ~= "" and result.stdout or nil
end

local function external_running()
    local result = mesh.exec("pgrep", { "-x", "wf-recorder" })
    return result.success
end

function start(self)
    mesh.service.set_poll_interval(POLL_MS)
    if external_running() then
        state = { available = true, recording = true, started_at = os.time(), output = "" }
    end
end

function on_poll(self)
    if owned then
        return
    end
    local running = external_running()
    if running ~= state.recording then
        state = {
            available = true,
            recording = running,
            started_at = running and os.time() or 0,
            output = state.output,
        }
    end
end

function on_stream_closed(self, program, _success)
    if program ~= "wf-recorder" then
        return
    end
    owned = false
    local output = state.output
    state = { available = true, recording = false, started_at = 0, output = output }
    if output ~= "" then
        self.RecordingSaved:fire({ path = output })
    end
end

function on_command_toggle(self)
    if state.recording then
        local result = mesh.exec("pkill", { "-INT", "-x", "wf-recorder" })
        return { ok = result.success, error = result.stderr }
    end
    local dir = videos_dir()
    if not dir then
        return { ok = false, error = "no videos directory" }
    end
    local stamp = tostring(os.time())
    local path = dir .. "/recording-" .. stamp .. ".mp4"
    if not mesh.exec_stream("wf-recorder", { "-f", path }) then
        return { ok = false, error = "wf-recorder failed to start" }
    end
    owned = true
    state = { available = true, recording = true, started_at = os.time(), output = path }
    return { ok = true, error = "" }
end
