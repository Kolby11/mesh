-- systemctl-backed session provider. logind authorizes the active local
-- session's user for these actions, so no privilege escalation is involved.

state = { available = true }

local function systemctl(action)
    local result = mesh.exec("systemctl", { action })
    return { ok = result.success, error = result.success and "" or (result.stderr or "") }
end

function start(self) end

function on_command_suspend(self)
    return systemctl("suspend")
end

function on_command_poweroff(self)
    return systemctl("poweroff")
end

function on_command_reboot(self)
    return systemctl("reboot")
end
