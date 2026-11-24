local function point(cx, cy, radius, degrees)
    local angle = math.rad(degrees)
    return cx + radius * math.sin(angle), cy - radius * math.cos(angle)
end

local function position(command, cx, cy, radius, degrees)
    local x, y = point(cx, cy, radius, degrees)
    return string.format("%s %.6f %.6f", command, x, y)
end

local function arc(cx, cy, radius, degrees, sweep)
    local x, y = point(cx, cy, radius, degrees)
    local direction = if sweep >= 0 then 1 else 0
    local large = if math.abs(sweep) > 180 then 1 else 0
    return string.format("A %.6f %.6f 0 %d %d %.6f %.6f", radius, radius, large, direction, x, y)
end

local function annular_sector(cx, cy, outer, inner, start, finish)
    assert(outer > 0 and inner >= 0 and inner < outer, "invalid annular sector radii")
    local sweep = math.clamp(finish - start, -360, 360)
    if sweep == 0 then return "" end
    finish = start + sweep
    local commands = { position("M", cx, cy, outer, start) }
    if math.abs(sweep) == 360 then
        commands[#commands + 1] = arc(cx, cy, outer, start + sweep / 2, sweep / 2)
    end
    local final_sweep = if math.abs(sweep) == 360 then sweep / 2 else sweep
    commands[#commands + 1] = arc(cx, cy, outer, finish, final_sweep)
    if inner == 0 then
        if math.abs(sweep) < 360 then
            commands[#commands + 1] = string.format("L %.6f %.6f", cx, cy)
        end
    else
        commands[#commands + 1] = position("L", cx, cy, inner, finish)
        if math.abs(sweep) == 360 then
            commands[#commands + 1] = arc(cx, cy, inner, start + sweep / 2, -sweep / 2)
        end
        commands[#commands + 1] = arc(cx, cy, inner, start, -final_sweep)
    end
    commands[#commands + 1] = "Z"
    return table.concat(commands, " ")
end

local function circle_arc(cx, cy, radius, start, finish)
    assert(radius > 0, "invalid circle radius")
    local sweep = math.clamp(finish - start, -360, 360)
    local first = position("M", cx, cy, radius, start)
    if math.abs(sweep) == 360 then
        return first .. " " .. arc(cx, cy, radius, start + sweep / 2, sweep / 2)
            .. " " .. arc(cx, cy, radius, start + sweep, sweep / 2)
    end
    return first .. " " .. arc(cx, cy, radius, start + sweep, sweep)
end

return table.freeze({
    point = point,
    annular_sector = annular_sector,
    circle_arc = circle_arc,
})
