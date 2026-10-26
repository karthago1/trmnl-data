use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, NaiveDate, Weekday};
use serde::Deserialize;
use serde_json::json;
use std::env;

const OPEN_METEO: &str = "https://api.open-meteo.com/v1/forecast";
const GEOCODING: &str = "https://geocoding-api.open-meteo.com/v1/search";

#[derive(Debug, Deserialize)]
struct GeocodingResponse {
    results: Option<Vec<Place>>,
}
#[derive(Debug, Deserialize)]
struct Place {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    admin1: Option<String>,
    #[serde(default)]
    country: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CurrentTemp {
    temperature_2m: f64,
}

#[derive(Debug, Deserialize)]
struct ForecastResponse {
    daily: DailyForecast,
    current: CurrentTemp,
}

#[derive(Debug, Deserialize)]
struct DailyForecast {
    time: Vec<String>,
    weather_code: Vec<i32>,
    temperature_2m_max: Vec<f64>,
    temperature_2m_min: Vec<f64>,
    precipitation_probability_max: Vec<Option<i32>>,
    wind_speed_10m_max: Vec<f64>,
}

#[derive(Debug, Clone)]
struct Day {
    date: NaiveDate,
    code: i32,
    high: f64,
    low: f64,
    temp: f64,
    rain_chance: Option<i32>,
    wind: f64,
}
#[derive(Debug)]
struct Weather {
    place: String,
    days: Vec<Day>,
}

async fn fetch_weather(city: &str) -> Result<Weather> {
    let client = reqwest::Client::builder()
        .user_agent("trmnl-data/0.1")
        .build()?;

    let geo: GeocodingResponse = client
        .get(GEOCODING)
        .query(&[
            ("name", city),
            ("count", "1"),
            ("language", "en"),
            ("format", "json"),
        ])
        .send()
        .await
        .context("Could not reach Open-Meteo geocoding")?
        .error_for_status()?
        .json()
        .await
        .context("Invalid geocoding response")?;

    let place = geo
        .results
        .and_then(|mut results| {
            if results.is_empty() {
                None
            } else {
                Some(results.remove(0))
            }
        })
        .ok_or_else(|| {
            anyhow!("No location found for '{city}'. Set CITY to a town or city name.")
        })?;

    let mut place_label = place.name.clone();
    if let Some(region) = place.admin1 {
        place_label.push_str(&format!(", {region}"));
    }
    if let Some(country) = place.country {
        place_label.push_str(&format!(", {country}"));
    }

    let forecast: ForecastResponse = client
        .get(OPEN_METEO)
        .query(&[
            ("latitude", place.latitude.to_string()),
            ("longitude", place.longitude.to_string()),
            ("daily", "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,wind_speed_10m_max".to_string()),
            ("forecast_days", "1".to_string()),
            ("current", "temperature_2m".to_string()),
            ("timezone", "auto".to_string()),
        ])
        .send().await.context("Could not fetch weather forecast")?
        .error_for_status()?.json().await.context("Invalid forecast response")?;

    let d = forecast.daily;
    let mut days = Vec::new();
    for i in 0..d.time.len().max(1) {
        days.push(Day {
            date: NaiveDate::parse_from_str(&d.time[i], "%Y-%m-%d")?,
            code: *d.weather_code.get(i).unwrap_or(&0),
            high: *d.temperature_2m_max.get(i).unwrap_or(&0.0),
            low: *d.temperature_2m_min.get(i).unwrap_or(&0.0),
            temp: forecast.current.temperature_2m,
            rain_chance: d.precipitation_probability_max.get(i).copied().flatten(),
            wind: *d.wind_speed_10m_max.get(i).unwrap_or(&0.0),
        });
    }
    Ok(Weather {
        place: place_label,
        days,
    })
}

fn weather_label(code: i32) -> &'static str {
    match code {
        0 => "CLEAR",
        1 => "MOSTLY CLEAR",
        2 => "PARTLY CLOUDY",
        3 => "OVERCAST",
        45 | 48 => "FOG",
        51 | 53 | 55 => "DRIZZLE",
        56 | 57 => "FREEZING DRIZZLE",
        61 | 63 | 65 => "RAIN",
        66 | 67 => "FREEZING RAIN",
        71 | 73 | 75 | 77 => "SNOW",
        80 | 81 | 82 => "RAIN SHOWERS",
        85 | 86 => "SNOW SHOWERS",
        95 => "THUNDERSTORM",
        96 | 99 => "THUNDER + HAIL",
        _ => "UNKNOWN",
    }
}
fn weather_symbol(code: i32) -> &'static str {
    match code {
        0 => "SUN",
        1 | 2 => "SUN/CLOUD",
        3 => "CLOUD",
        45 | 48 => "FOG",
        51..=67 => "RAIN",
        71..=77 | 85..=86 => "SNOW",
        80..=82 => "SHOWERS",
        95..=99 => "STORM",
        _ => "WEATHER",
    }
}
fn day_name(date: NaiveDate, index: usize) -> String {
    if index == 0 {
        "TODAY".to_string()
    } else if index == 1 {
        "TOMORROW".to_string()
    } else {
        weekday_name(date.weekday()).to_string()
    }
}
fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "MONDAY",
        Weekday::Tue => "TUESDAY",
        Weekday::Wed => "WEDNESDAY",
        Weekday::Thu => "THURSDAY",
        Weekday::Fri => "FRIDAY",
        Weekday::Sat => "SATURDAY",
        Weekday::Sun => "SUNDAY",
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let city = env::var("CITY").unwrap_or_else(|_| "Neukirch".to_string());
    let weather = fetch_weather(&city).await?;
    let updated = chrono::Local::now().format("%d.%m.%Y %H:%M").to_string();
    let d = &weather.days[0];
    let data = json!({
        "date": updated,
        "temp": d.temp,
        "min_temp": d.low,
        "max_temp": d.high,
        "rain": d.rain_chance.unwrap_or(0)
    });
    println!("{data}");
    Ok(())
}
