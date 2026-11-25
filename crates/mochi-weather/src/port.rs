//! Pure weather parsing, coordinate rounding, and forecast models.

#![forbid(unsafe_code)]

use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct HourlyWeatherPoint {
    pub label: String,
    pub temp: f64,
    pub rain_chance: i64,
    pub code: i64,
    pub is_day: bool,
}

#[derive(Debug, Clone)]
pub struct WeatherReport {
    pub temperature: f64,
    pub feels_like: Option<f64>,
    pub humidity: Option<i64>,
    pub wind_kmh: Option<f64>,
    pub uv_index: Option<f64>,
    pub code: i64,
    pub is_day: bool,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub rain_at: Option<NaiveDateTime>,
    pub rain_chance: i64,
    pub icon: String,
    pub description: String,
    pub temperature_text: String,
    pub hourly: Vec<HourlyWeatherPoint>,
    pub fetched_at: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct WeatherPlace {
    pub name: String,
    pub detail: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone)]
pub struct ParsedWeatherReport {
    pub temperature: f64,
    pub code: i64,
    pub is_day: bool,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub rain_at: Option<NaiveDateTime>,
    pub rain_chance: i64,
    pub icon: &'static str,
    pub temperature_text: String,
}

pub fn parse_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .ok()
}

pub fn weather_icon_name(code: i64, is_day: bool) -> &'static str {
    match code {
        0 | 1 => {
            if is_day {
                "Clear"
            } else {
                "ClearNight"
            }
        }
        2 => "PartlyCloudy",
        3 => "Cloudy",
        45 | 48 => "Fog",
        51..=67 | 80..=82 => "Rain",
        71..=77 | 85 | 86 => "Snow",
        95.. => "Storm",
        _ => "Cloudy",
    }
}

fn try_number(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn first_number(v: &Value, key: &str) -> Option<f64> {
    v.get(key)
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_f64)
}

pub fn parse_weather_report(root: &Value, now: NaiveDateTime) -> Option<ParsedWeatherReport> {
    let current = root.get("current")?;
    let (temperature, code_f) = (
        try_number(current, "temperature_2m")?,
        try_number(current, "weather_code")?,
    );
    let code = code_f as i64;
    let is_day = try_number(current, "is_day").is_none_or(|d| d >= 1.0);

    let (high, low) = match root.get("daily") {
        Some(daily) => (
            first_number(daily, "temperature_2m_max"),
            first_number(daily, "temperature_2m_min"),
        ),
        None => (None, None),
    };

    let mut rain_at: Option<NaiveDateTime> = None;
    let mut rain_chance: i64 = 0;
    if let Some(hourly) = root.get("hourly")
        && let (Some(times), Some(chances)) = (
            hourly.get("time").and_then(Value::as_array),
            hourly
                .get("precipitation_probability")
                .and_then(Value::as_array),
        )
    {
        let hour = now.date().and_hms_opt(now.hour(), 0, 0).unwrap();
        let window_end = now + Duration::hours(2);
        let count = times.len().min(chances.len());
        for i in 0..count {
            let Some(ts) = times[i].as_str().and_then(parse_dt) else {
                continue;
            };
            if ts < hour || ts > window_end {
                continue;
            }
            if let Some(ch) = chances[i].as_i64()
                && ch >= 60
            {
                rain_at = Some(ts);
                rain_chance = ch;
                break;
            }
        }
    }

    let temperature_text = format!("{}°", temperature.round_ties_even() as i64);
    Some(ParsedWeatherReport {
        temperature,
        code,
        is_day,
        high,
        low,
        rain_at,
        rain_chance,
        icon: weather_icon_name(code, is_day),
        temperature_text,
    })
}

pub fn parse_weather_places(json_str: &str) -> Vec<WeatherPlace> {
    let Ok(root) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };
    let Some(results) = root.get("results").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut places = Vec::new();
    for r in results {
        let (Some(lat), Some(lon), Some(name)) = (
            try_number(r, "latitude"),
            try_number(r, "longitude"),
            r.get("name").and_then(Value::as_str),
        ) else {
            continue;
        };
        let parts: Vec<&str> = [
            r.get("admin1").and_then(Value::as_str),
            r.get("country").and_then(Value::as_str),
        ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
        places.push(WeatherPlace {
            name: name.to_string(),
            detail: parts.join(", "),
            latitude: lat,
            longitude: lon,
        });
    }
    places
}

pub fn coarse_coordinate(coordinate: f64) -> f64 {
    if coordinate.is_nan() || coordinate.is_infinite() || coordinate.abs() > 1e15 {
        return coordinate;
    }
    (coordinate * 100.0).round_ties_even() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_meteo_report_and_rounds_coordinates() {
        assert_eq!(coarse_coordinate(-6.2088), -6.21);
        assert_eq!(weather_icon_name(0, false), "ClearNight");

        let raw = r#"{"current":{"temperature_2m":29.4,"weather_code":2,"is_day":1},"daily":{"temperature_2m_max":[32.0],"temperature_2m_min":[25.0]}}"#;
        let root: Value = serde_json::from_str(raw).unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 24)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let rep = parse_weather_report(&root, now).unwrap();
        assert_eq!(rep.temperature_text, "29°");
        assert_eq!(rep.icon, "PartlyCloudy");
    }
}
