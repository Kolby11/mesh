-- Volume presentation shared by every audio frontend, so the bar, quick
-- settings and the settings page show the same glyph for the same level.
local volume = {}

-- An integer level in 0..100; non-numbers, NaN and infinities read as 0.
function volume.percent(value)
    local number = tonumber(value)
    if number == nil or number ~= number or number == math.huge or number == -math.huge then
        return 0
    end
    return math.clamp(math.floor(number), 0, 100)
end

-- The semantic icon for a level and mute state.
function volume.icon(percent, muted)
    local level = volume.percent(percent)
    if muted or level == 0 then
        return "audio-volume-muted"
    elseif level < 34 then
        return "audio-volume-low"
    elseif level < 67 then
        return "audio-volume-medium"
    end
    return "audio-volume-high"
end

-- The level after one scroll step of `amount` percent in the direction of `dy`.
function volume.step(percent, dy, amount)
    if dy == 0 then
        return volume.percent(percent)
    end
    return volume.percent(volume.percent(percent) + (dy > 0 and amount or -amount))
end

return volume
