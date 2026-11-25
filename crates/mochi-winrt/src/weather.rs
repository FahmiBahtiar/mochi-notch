use chrono::{Local, NaiveDateTime, Timelike};
use serde_json::Value;

pub use mochi_weather::port::{HourlyWeatherPoint, WeatherPlace, WeatherReport};

use crate::http::{http_request, url_encode};
use crate::strings::Language;

fn parse_open_meteo_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()
}

pub fn fetch_weather(lat: f64, lon: f64, lang: Language) -> Option<WeatherReport> {
    let c_lat = mochi_weather::port::coarse_coordinate(lat).unwrap_or(lat);
    let c_lon = mochi_weather::port::coarse_coordinate(lon).unwrap_or(lon);
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={c_lat:.2}&longitude={c_lon:.2}&current=temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,weather_code,is_day&hourly=temperature_2m,precipitation_probability,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min,uv_index_max&timezone=auto&forecast_days=2"
    );
    let resp = http_request("GET", &url, &[], None, 15_000).ok()?;
    if !(200..300).contains(&resp.status) {
        return None;
    }
    let now = Local::now().naive_local();
    let now_iso = now.format("%Y-%m-%dT%H:%M:%S").to_string();
    let root: Value = serde_json::from_str(&resp.body).ok()?;
    let parsed = mochi_weather::port::parse_weather_report(resp.body, now_iso).ok()??;
    let code = parsed.get("Code")?.as_i64()?;
    let is_day = parsed.get("IsDay")?.as_bool()?;
    let temp = parsed.get("Temperature")?.as_f64()?;
    let rain_at = parsed
        .get("RainAtIso")
        .and_then(Value::as_str)
        .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok());

    let current = root.get("current");
    let feels_like = current
        .and_then(|c| c.get("apparent_temperature"))
        .and_then(Value::as_f64);
    let humidity = current
        .and_then(|c| c.get("relative_humidity_2m"))
        .and_then(Value::as_f64)
        .map(|v| v.round() as i64);
    let wind_kmh = current
        .and_then(|c| c.get("wind_speed_10m"))
        .and_then(Value::as_f64);
    let uv_index = root
        .get("daily")
        .and_then(|d| d.get("uv_index_max"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_f64);

    let mut hourly_pts = Vec::new();
    if let Some(hourly) = root.get("hourly")
        && let (Some(times), Some(temps)) = (
            hourly.get("time").and_then(Value::as_array),
            hourly.get("temperature_2m").and_then(Value::as_array),
        )
    {
        let chances = hourly
            .get("precipitation_probability")
            .and_then(Value::as_array);
        let codes = hourly.get("weather_code").and_then(Value::as_array);
        let days = hourly.get("is_day").and_then(Value::as_array);
        let cur_hour = now.date().and_hms_opt(now.hour(), 0, 0).unwrap_or(now);
        let start_idx = times
            .iter()
            .position(|t| {
                t.as_str()
                    .and_then(parse_open_meteo_dt)
                    .is_some_and(|dt| dt >= cur_hour)
            })
            .unwrap_or(0);
        let s = lang.strings();
        for step in 0..6 {
            let idx = start_idx + step * 3;
            if idx >= times.len() || idx >= temps.len() {
                break;
            }
            let Some(dt) = times[idx].as_str().and_then(parse_open_meteo_dt) else {
                continue;
            };
            let Some(htemp) = temps[idx].as_f64() else {
                continue;
            };
            let hrain = chances
                .and_then(|a| a.get(idx))
                .and_then(Value::as_f64)
                .map_or(0, |v| v.round() as i64);
            let hcode = codes
                .and_then(|a| a.get(idx))
                .and_then(Value::as_f64)
                .map_or(code, |v| v as i64);
            let hday = days
                .and_then(|a| a.get(idx))
                .and_then(Value::as_f64)
                .is_none_or(|v| v >= 1.0);
            let label = if step == 0 {
                s.weather_now.to_string()
            } else {
                format!("{:02}:00", dt.hour())
            };
            hourly_pts.push(HourlyWeatherPoint {
                label,
                temp: htemp,
                rain_chance: hrain,
                code: hcode,
                is_day: hday,
            });
        }
    }

    Some(WeatherReport {
        temperature: temp,
        feels_like,
        humidity,
        wind_kmh,
        uv_index,
        code,
        is_day,
        high: parsed.get("High").and_then(Value::as_f64),
        low: parsed.get("Low").and_then(Value::as_f64),
        rain_at,
        rain_chance: parsed
            .get("RainChance")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        icon: parsed
            .get("Icon")
            .and_then(Value::as_str)
            .unwrap_or("Cloudy")
            .to_string(),
        description: describe_weather(code, lang).to_string(),
        temperature_text: parsed
            .get("TemperatureText")
            .and_then(Value::as_str)
            .unwrap_or("0°")
            .to_string(),
        hourly: hourly_pts,
        fetched_at: now,
    })
}

pub fn search_weather_places(query: &str, lang: Language) -> Vec<WeatherPlace> {
    let lang_code = match lang {
        Language::Id => "id",
        Language::En => "en",
    };
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?count=6&language={lang_code}&format=json&name={}",
        url_encode(query.trim())
    );
    let Ok(resp) = http_request("GET", &url, &[], None, 15_000) else {
        return Vec::new();
    };
    if !(200..300).contains(&resp.status) {
        return Vec::new();
    }
    let Ok(list) = mochi_weather::port::parse_weather_places(resp.body) else {
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|v| {
            Some(WeatherPlace {
                name: v.get("Name")?.as_str()?.to_string(),
                detail: v.get("Detail")?.as_str()?.to_string(),
                latitude: v.get("Latitude")?.as_f64()?,
                longitude: v.get("Longitude")?.as_f64()?,
            })
        })
        .collect()
}

pub fn describe_weather(code: i64, lang: Language) -> &'static str {
    let s = lang.strings();
    match code {
        0 => s.weather_clear,
        1 => s.weather_mostly_clear,
        2 => s.weather_partly_cloudy,
        3 => s.weather_cloudy,
        45 | 48 => s.weather_fog,
        51 | 53 | 55 => s.weather_drizzle,
        56 | 57 => s.weather_freezing_drizzle,
        61 => s.weather_light_rain,
        63 => s.weather_rain,
        65 => s.weather_heavy_rain,
        66 | 67 => s.weather_freezing_rain,
        71 | 73 | 75 | 77 => s.weather_snow,
        80 | 81 => s.weather_rain_showers,
        82 => s.weather_heavy_showers,
        85 | 86 => s.weather_snow_showers,
        95 => s.weather_thunderstorm,
        96 | 99 => s.weather_thunderstorm_hail,
        _ => "—",
    }
}
