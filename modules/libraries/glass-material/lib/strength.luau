local TIERS = {
    pill = { blur = 16, depth = 6, edge = 5, dispersion = 0.4 },
    card = { blur = 24, depth = 10, edge = 8, dispersion = 0.6 },
    dock = { blur = 20, depth = 9, edge = 7, dispersion = 0.6 },
    panel = { blur = 32, depth = 12, edge = 9, dispersion = 0.6 },
    flare = { blur = 10, depth = 0, edge = 0, dispersion = 0 },
    lens = { blur = 0, depth = 8, edge = 8, dispersion = 0 },
}

local function length(value)
    return string.format("%gpx", math.floor(value * 10 + 0.5) / 10)
end

local function backdrop_filter(tier, blur, liquid)
    local parts = {}
    if tier.blur > 0 then
        parts[#parts + 1] = "blur(" .. length(tier.blur * blur / 100) .. ")"
    end
    parts[#parts + 1] = "refract(" .. length(tier.depth * liquid / 100) .. " "
        .. length(tier.edge * liquid / 100) .. ")"
    if tier.dispersion > 0 and liquid > 0 then
        parts[#parts + 1] = "dispersion(" .. length(tier.dispersion * liquid / 100) .. ")"
    end
    return table.concat(parts, " ")
end

return function(blur, liquid, fallback)
    local tokens = {}
    for name, tier in pairs(TIERS) do
        tokens["@mesh/glass-material.glass.backdrop." .. name] =
            (blur ~= nil and liquid ~= nil) and backdrop_filter(tier, blur, liquid) or fallback
    end
    return tokens
end
