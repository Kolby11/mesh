-- Open-Meteo weather provider.
--
-- Coordinates come from settings; without them the provider locates the host
-- once by IP. Forecast points are aligned to multiples of the configured step
-- in local time (18:00, 21:00, ...), so the widget never shows odd hours.

local FORECAST_POINTS = 4
local RETRY_MS = 60000

state = {
    available = false, location = "", temperature = 0, feels_like = 0,
    unit = "C", condition = "cloudy", is_day = true, hourly = {}, updated_at = 0,
}

local located = nil

local function settings()
    local ok, value = pcall(mesh.config)
    return ok and type(value) == "table" and value or {}
end

local function fetch_json(url)
    local result = mesh.http_request({ url = url, timeout_ms = 15000 })
    if not result.success then
        return nil
    end
    local ok, value = pcall(mesh.json.decode, result.body or "")
    return ok and type(value) == "table" and value or nil
end

-- WMO weather interpretation codes -> the interface's semantic conditions.
local function condition_for(code)
    code = tonumber(code) or 3
    if code == 0 then return "clear" end
    if code <= 2 then return "partly-cloudy" end
    if code == 3 then return "cloudy" end
    if code == 45 or code == 48 then return "fog" end
    if code >= 51 and code <= 57 then return "drizzle" end
    if (code >= 61 and code <= 67) or (code >= 80 and code <= 82) then return "rain" end
    if (code >= 71 and code <= 77) or code == 85 or code == 86 then return "snow" end
    if code >= 95 then return "storm" end
    return "cloudy"
end

local function locate(config)
    local lat, lon = tonumber(config.latitude), tonumber(config.longitude)
    if lat and lon then
        return { latitude = lat, longitude = lon, name = tostring(config.location or "") }
    end
    if located then
        return located
    end
    local info = fetch_json("https://ipinfo.io/json")
    local ip_lat, ip_lon = tostring(info and info.loc or ""):match("^([%-%d%.]+),([%-%d%.]+)$")
    if not ip_lat then
        return nil
    end
    located = { latitude = tonumber(ip_lat), longitude = tonumber(ip_lon), name = tostring(info.city or "") }
    return located
end

local function refresh()
    local config = settings()
    local place = locate(config)
    if not place then
        mesh.log.warn("[open-meteo-weather] no location; set latitude/longitude in settings")
        return false
    end
    local unit = config.unit == "F" and "F" or "C"
    local step = math.clamp(math.floor(tonumber(config.forecast_step_hours) or 3), 1, 6)
    local url = string.format(
        "https://api.open-meteo.com/v1/forecast?latitude=%.4f&longitude=%.4f"
            .. "&current=temperature_2m,apparent_temperature,weather_code,is_day"
            .. "&hourly=temperature_2m,weather_code&forecast_hours=%d"
            .. "&timezone=auto&timeformat=unixtime&temperature_unit=%s",
        place.latitude, place.longitude, step * FORECAST_POINTS + 2,
        unit == "F" and "fahrenheit" or "celsius"
    )
    local data = fetch_json(url)
    local current = data and data.current
    if type(current) ~= "table" then
        mesh.log.warn("[open-meteo-weather] forecast request failed")
        return false
    end

    local offset = tonumber(data.utc_offset_seconds) or 0
    local hourly = {}
    local times = data.hourly and data.hourly.time or {}
    local temps = data.hourly and data.hourly.temperature_2m or {}
    local codes = data.hourly and data.hourly.weather_code or {}
    for index, time in times do
        local local_hour = math.floor(((time + offset) % 86400) / 3600)
        if time > (current.time or 0) and local_hour % step == 0 and #hourly < FORECAST_POINTS then
            hourly[#hourly + 1] = {
                time = time,
                temperature = tonumber(temps[index]) or 0,
                condition = condition_for(codes[index]),
            }
        end
    end

    state = {
        available = true,
        location = place.name,
        temperature = tonumber(current.temperature_2m) or 0,
        feels_like = tonumber(current.apparent_temperature) or 0,
        unit = unit,
        condition = condition_for(current.weather_code),
        is_day = current.is_day ~= 0,
        hourly = hourly,
        updated_at = os.time(),
    }
    return true
end

local function schedule(ok)
    local minutes = math.max(5, tonumber(settings().interval_minutes) or 15)
    mesh.service.set_poll_interval(ok and minutes * 60000 or RETRY_MS)
end

function start(self)
    schedule(refresh())
end

function on_poll(self)
    schedule(refresh())
end

function on_command_refresh(self)
    local ok = refresh()
    schedule(ok)
    return { ok = ok, error = ok and "" or "forecast request failed" }
end
