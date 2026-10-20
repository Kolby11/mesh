-- Wallpaper library provider.
--
-- Listing is one cheap process; previews are made afterwards, one image per
-- process so each stays inside the exec timeout, and published as they land.
-- Previews are cached by path and modification time.

local LIST = "d=\"$1\"; [ \"$d\" = \"~\" ] && d=\"$HOME\"; [ \"${d#\"~/\"}\" != \"$d\" ] && d=\"$HOME/${d#\"~/\"}\"; [ -d \"$d\" ] || exit 1; printf '%s\\n' \"$d\"; find \"$d\" -maxdepth 1 -type f"
local THUMB = "f=\"$1\"; t=\"${XDG_CACHE_HOME:-$HOME/.cache}/mesh/wallpaper-thumbs\"; mkdir -p \"$t\" || exit 1; k=\"$(printf %s \"$f\" | cksum | cut -d' ' -f1)-$(stat -c %Y \"$f\")\"; [ -f \"$t/$k.jpg\" ] || magick \"$f[0]\" -thumbnail 640x400^ -gravity center -extent 640x400 -quality 82 \"$t/$k.jpg\" || exit 1; [ -f \"$t/$k.color\" ] || magick \"$t/$k.jpg\" -resize 1x1! -format '%[hex:p{0,0}]' info: > \"$t/$k.color\"; printf '%s\\t%s' \"$t/$k.jpg\" \"$(cut -c1-6 \"$t/$k.color\")\""
local SAFETY_POLL_MS = 60000
local IMAGE_EXTENSIONS = { jpg = true, jpeg = true, png = true, webp = true }

state = { available = false, directory = "", wallpapers = {} }

local previews = {}
local generation = 0

local function directory_setting()
    local ok, config = pcall(mesh.config)
    local value = ok and type(config) == "table" and config.directory
    return type(value) == "string" and value ~= "" and value or "~/Pictures/Wallpapers"
end

local function entry(path)
    local preview = previews[path]
    return {
        path = path,
        name = (path:match("([^/]+)$") or path):gsub("%.[^.]+$", ""),
        thumbnail = preview and preview.thumbnail or path,
        color = preview and preview.color or "",
    }
end

local function scan()
    generation += 1
    local current = generation
    local result = mesh.exec("sh", { "-c", LIST, "sh", directory_setting() })
    if not result.success then
        state = { available = false, directory = "", wallpapers = {} }
        return
    end
    local lines = {}
    for line in (result.stdout or ""):gmatch("[^\n]+") do
        lines[#lines + 1] = line
    end
    local directory = table.remove(lines, 1) or ""
    local images = {}
    for _, path in lines do
        local extension = string.lower(path:match("%.([^./]+)$") or "")
        if IMAGE_EXTENSIONS[extension] then
            images[#images + 1] = path
        end
    end
    lines = images
    table.sort(lines, function(a, b)
        return string.lower(a) < string.lower(b)
    end)
    local list = {}
    for _, path in lines do
        list[#list + 1] = entry(path)
    end
    state = { available = true, directory = directory, wallpapers = list }

    task.spawn(function()
        for index, path in lines do
            if current ~= generation then
                return
            end
            if not previews[path] then
                local made = mesh.exec("sh", { "-c", THUMB, "sh", path })
                local thumbnail, color = (made.stdout or ""):match("^([^\t]+)\t(%x*)$")
                previews[path] = {
                    thumbnail = made.success and thumbnail or path,
                    color = made.success and color ~= "" and ("#" .. string.lower(color)) or "",
                }
                if current == generation then
                    local updated = table.clone(state.wallpapers)
                    updated[index] = entry(path)
                    state = { available = true, directory = directory, wallpapers = updated }
                end
            end
        end
    end)
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
