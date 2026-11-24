local kinds = table.freeze({ path = true, rect = true, text = true, image = true, icon = true })
local max_commands = 256
local prepare

local function decoded_text(value, maximum)
    if value == nil then return "" end
    assert(type(value) == "string" and #value <= maximum, "invalid drawing string")
    return value
end

local function coordinate(value, minimum)
    assert(
        type(value) == "number" and value >= minimum and value <= 16384,
        "drawing coordinates must be finite and within bounds"
    )
    return value
end

local function position(item)
    return string.format(
        "left: %.6fpx; top: %.6fpx; width: %.6fpx; height: %.6fpx;",
        coordinate(item.x or 0, -16384),
        coordinate(item.y or 0, -16384),
        coordinate(item.width, 0.01),
        coordinate(item.height, 0.01)
    )
end

local function decode(item, decoration, budget)
    assert(type(item) == "table" and kinds[item.kind], "invalid drawing command")
    local shape = item.kind == "path" or item.kind == "rect"
    local label = decoded_text(item.label, 1024)
    assert(label == "" or (shape and not decoration), "drawing regions require a path or rectangle")
    local data = item.kind == "rect" and "M0 0H100V100H0Z" or decoded_text(item.d, 16384)
    assert(not shape or data ~= "", "path drawing requires path data")
    local rule = item.fill_rule or "nonzero"
    local fill = item.path_fill or "auto"
    assert(rule == "nonzero" or rule == "evenodd", "invalid drawing fill rule")
    assert(fill == "auto" or fill == "none", "invalid drawing path fill")
    assert(item.disabled == nil or type(item.disabled) == "boolean", "invalid drawing disabled state")
    assert(item.art == nil or (shape and not decoration), "drawing art requires a path or rectangle")

    return {
        kind = item.kind,
        shape = shape,
        d = data,
        view_box = item.kind == "rect" and "0 0 100 100" or decoded_text(item.view_box or "0 0 100 100", 128),
        fill_rule = rule,
        path_fill = fill,
        label = label,
        disabled = item.disabled == true,
        style = decoded_text(item.style, 4096) .. ";" .. position(item),
        class = decoded_text(item.class, 256),
        text = decoded_text(item.text, 4096),
        text_path = decoded_text(item.text_path, 16384),
        src = decoded_text(item.src, 4096),
        alt = decoded_text(item.alt, 1024),
        name = decoded_text(item.name, 256),
        icon_size = item.kind == "icon" and coordinate(math.min(item.width, item.height), 0.01) or 0,
        art = item.art and prepare(item.art, true, budget) or {},
    }
end

prepare = function(commands, decoration, budget)
    assert(type(commands) == "table" and #commands <= budget.remaining, "drawing command limit exceeded")
    budget.remaining -= #commands
    local output, ids = {}, {}
    for index, command in commands do
        assert(type(index) == "number" and index % 1 == 0 and index >= 1 and index <= #commands, "drawing commands must form a dense array")
        assert(type(command) == "table", "invalid drawing command")
        local id = decoded_text(command.id, 128)
        assert(id ~= "" and not ids[id], "drawing command IDs must be unique and nonempty")
        local record = decode(command, decoration, budget)
        ids[id] = true
        record.id = id
        output[index] = record
    end
    for index = 1, #commands do
        assert(output[index] ~= nil, "drawing commands must form a dense array")
    end
    return output
end

return table.freeze({
    prepare = function(commands)
        return prepare(commands, false, { remaining = max_commands })
    end,
})
