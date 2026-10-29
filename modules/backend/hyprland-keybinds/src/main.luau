-- Hyprland keybind provider.
--
-- Reads the Lua config, not the compositor: `hyprctl binds` has no section
-- titles. A `--##! Name` comment starts a section; an `hl.bind(...)` or
-- `replace_bind(...)` call (it may span lines) becomes a row when its options carry a `description`.
-- Undescribed binds are left out, as they are in a cheatsheet.

local READ = "for f in \"$HOME/.config/hypr/hyprland/keybinds.lua\" \"$HOME/.config/hypr/custom/keybinds.lua\"; do [ -f \"$f\" ] && { cat \"$f\"; echo; }; done; true"
local SAFETY_POLL_MS = 60000

state = { available = false, sections = {} }

local function depth(text)
    local bare = text:gsub("\"[^\"]*\"", "\"\"")
    local _, opens = bare:gsub("%(", "")
    local _, closes = bare:gsub("%)", "")
    return opens - closes
end

-- "SUPER + SHIFT + Slash" -> { "Super", "Shift" }, "Slash"
local function chord(text)
    local parts = {}
    for part in text:gmatch("[^+]+") do
        part = part:match("^%s*(.-)%s*$")
        if part ~= "" then
            parts[#parts + 1] = part
        end
    end
    local key = table.remove(parts) or ""
    local mods = {}
    for _, mod in parts do
        mods[#mods + 1] = mod:sub(1, 1):upper() .. mod:sub(2):lower()
    end
    return mods, key
end

local function parse(source)
    local sections, by_name = {}, {}
    local current = "Uncategorised"
    local pending, open = nil, 0

    local function add(call)
        local keys = call:match("^[%w_%.]*bind%(%s*\"([^\"]*)\"")
        local comment = call:match("description%s*=%s*\"([^\"]*)\"")
        if not keys or not comment then
            return
        end
        local prefix = current .. ": "
        if comment:sub(1, #prefix) == prefix then
            comment = comment:sub(#prefix + 1)
        end
        local mods, key = chord(keys)
        local section = by_name[current]
        if not section then
            section = { name = current, binds = {} }
            by_name[current] = section
            sections[#sections + 1] = section
        end
        section.binds[#section.binds + 1] = { mods = mods, key = key, comment = comment }
    end

    for line in source:gmatch("[^\n]*") do
        if pending then
            pending = pending .. " " .. line
            open += depth(line)
            if open <= 0 then
                add(pending)
                pending = nil
            end
        else
            local title = line:match("^%s*%-%-#+!%s*(.-)%s*$")
            if title and title ~= "" then
                current = title
            elseif line:match("^%s*[%w_%.]*bind%(") then
                local call = line:match("^%s*(.-)%s*$")
                open = depth(call)
                if open <= 0 then
                    add(call)
                else
                    pending = call
                end
            end
        end
    end
    return sections
end

local function scan()
    local result = mesh.exec("sh", { "-c", READ })
    if not result.success then
        state = { available = false, sections = {} }
        return
    end
    local sections = parse(result.stdout or "")
    state = { available = #sections > 0, sections = sections }
end

function start(self)
    scan()
    mesh.service.set_poll_interval(SAFETY_POLL_MS)
end

function on_poll(self)
    scan()
end

function on_command_refresh(self)
    scan()
    return { ok = true }
end
