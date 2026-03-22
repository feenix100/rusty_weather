//! Weather API orchestration for Open-Meteo forecast/archive queries.
//!
//! This module is intentionally verbose and instructional so it can support tutorial usage.

use crate::date_utils;
use crate::geocoding_service;
use crate::models::{LocationQuery, WeatherDayRecord, WeatherQueryRequest, WeatherQueryResult};
use crate::weather_code_map;
use chrono::{Local, NaiveDate};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    timezone: Option<String>,
    current: Option<OpenMeteoCurrent>,
    daily: Option<OpenMeteoDaily>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: Option<f64>,
    apparent_temperature: Option<f64>,
    weather_code: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoDaily {
    time: Vec<String>,
    temperature_2m_max: Option<Vec<f64>>,
    temperature_2m_min: Option<Vec<f64>>,
    precipitation_sum: Option<Vec<f64>>,
    wind_speed_10m_max: Option<Vec<f64>>,
    weather_code: Option<Vec<i32>>,
}

#[derive(Debug, Clone)]
struct PartialWeatherPayload {
    timezone: String,
    current_temperature_c: Option<f64>,
    current_apparent_temperature_c: Option<f64>,
    current_weather_code: Option<i32>,
    days: Vec<WeatherDayRecord>,
}

/// Fetches weather for the full user request, handling:
/// - city/state geocoding
/// - coordinate mode
/// - archive vs forecast endpoint selection
/// - split ranges that cross today's date
pub fn fetch_weather(request: WeatherQueryRequest) -> Result<WeatherQueryResult, String> {
    let (city, state, lat, lon, tz_hint) = match &request.location {
        LocationQuery::CityState { city, state } => {
            let resolved = geocoding_service::geocode_city_state(city, state)?;
            (
                Some(resolved.city),
                Some(resolved.state),
                resolved.latitude,
                resolved.longitude,
                resolved.timezone,
            )
        }
        LocationQuery::Coordinates {
            latitude,
            longitude,
        } => {
            validate_coordinates(*latitude, *longitude)?;
            (None, None, *latitude, *longitude, None)
        }
    };

    let payload = fetch_weather_range(lat, lon, request.start_date, request.end_date, tz_hint)?;

    let current_weather_description = payload
        .current_weather_code
        .map(|code| weather_code_map::map_weather_code(code).0.to_owned());

    Ok(WeatherQueryResult {
        fetched_at: Local::now(),
        city,
        state,
        latitude: lat,
        longitude: lon,
        timezone: payload.timezone,
        start_date: request.start_date.format("%Y-%m-%d").to_string(),
        end_date: request.end_date.format("%Y-%m-%d").to_string(),
        current_temperature_c: payload.current_temperature_c,
        current_apparent_temperature_c: payload.current_apparent_temperature_c,
        current_weather_code: payload.current_weather_code,
        current_weather_description,
        days: payload.days,
    })
}

fn validate_coordinates(lat: f64, lon: f64) -> Result<(), String> {
    if !(-90.0..=90.0).contains(&lat) {
        return Err("Latitude must be between -90 and 90.".to_owned());
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err("Longitude must be between -180 and 180.".to_owned());
    }
    Ok(())
}

fn fetch_weather_range(
    latitude: f64,
    longitude: f64,
    start_date: NaiveDate,
    end_date: NaiveDate,
    timezone_hint: Option<String>,
) -> Result<PartialWeatherPayload, String> {
    let today = Local::now().date_naive();

    if end_date < today {
        return fetch_single_endpoint(
            EndpointKind::Archive,
            latitude,
            longitude,
            start_date,
            end_date,
            timezone_hint,
        );
    }

    if start_date >= today {
        return fetch_single_endpoint(
            EndpointKind::Forecast,
            latitude,
            longitude,
            start_date,
            end_date,
            timezone_hint,
        );
    }

    // If the user spans past and present/future, we split into archive + forecast.
    let archive_end = today.pred_opt().ok_or_else(|| {
        "Could not build split date range for archive/forecast boundary.".to_owned()
    })?;

    let archive_part = fetch_single_endpoint(
        EndpointKind::Archive,
        latitude,
        longitude,
        start_date,
        archive_end,
        timezone_hint.clone(),
    )?;

    let forecast_part = fetch_single_endpoint(
        EndpointKind::Forecast,
        latitude,
        longitude,
        today,
        end_date,
        timezone_hint,
    )?;

    let timezone = if !forecast_part.timezone.is_empty() {
        forecast_part.timezone.clone()
    } else {
        archive_part.timezone.clone()
    };

    let mut by_date = BTreeMap::<String, WeatherDayRecord>::new();
    for day in archive_part.days.into_iter().chain(forecast_part.days.clone()) {
        by_date.insert(day.day_date.clone(), day);
    }

    Ok(PartialWeatherPayload {
        timezone,
        current_temperature_c: forecast_part.current_temperature_c,
        current_apparent_temperature_c: forecast_part.current_apparent_temperature_c,
        current_weather_code: forecast_part.current_weather_code,
        days: by_date.into_values().collect(),
    })
}

#[derive(Clone, Copy)]
enum EndpointKind {
    Forecast,
    Archive,
}

fn fetch_single_endpoint(
    kind: EndpointKind,
    latitude: f64,
    longitude: f64,
    start_date: NaiveDate,
    end_date: NaiveDate,
    timezone_hint: Option<String>,
) -> Result<PartialWeatherPayload, String> {
    let endpoint = match kind {
        EndpointKind::Forecast => "https://api.open-meteo.com/v1/forecast",
        EndpointKind::Archive => "https://archive-api.open-meteo.com/v1/archive",
    };

    let source_label = match kind {
        EndpointKind::Forecast => "forecast",
        EndpointKind::Archive => "archive",
    }
    .to_owned();

    let tz_value = timezone_hint.as_deref().unwrap_or("auto");

    let client = reqwest::blocking::Client::new();
    let mut req = client.get(endpoint).query(&[
        ("latitude", latitude.to_string()),
        ("longitude", longitude.to_string()),
        (
            "daily",
            "weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum,wind_speed_10m_max"
                .to_owned(),
        ),
        (
            "start_date",
            start_date.format("%Y-%m-%d").to_string(),
        ),
        ("end_date", end_date.format("%Y-%m-%d").to_string()),
        ("timezone", tz_value.to_owned()),
    ]);

    if matches!(kind, EndpointKind::Forecast) {
        req = req.query(&[(
            "current",
            "temperature_2m,apparent_temperature,weather_code".to_owned(),
        )]);
    }

    let response = req
        .send()
        .map_err(|e| format!("Weather request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Weather API returned error: {e}"))?;

    let payload: OpenMeteoResponse = response
        .json()
        .map_err(|e| format!("Failed to parse weather JSON: {e}"))?;

    let daily = payload
        .daily
        .ok_or_else(|| "Weather API did not return daily fields.".to_owned())?;

    let days = build_day_records(daily, &source_label)?;

    Ok(PartialWeatherPayload {
        timezone: payload.timezone.unwrap_or_else(|| "unknown".to_owned()),
        current_temperature_c: payload.current.as_ref().and_then(|c| c.temperature_2m),
        current_apparent_temperature_c: payload.current.as_ref().and_then(|c| c.apparent_temperature),
        current_weather_code: payload.current.and_then(|c| c.weather_code),
        days,
    })
}

fn build_day_records(daily: OpenMeteoDaily, source: &str) -> Result<Vec<WeatherDayRecord>, String> {
    let len = daily.time.len();
    if len == 0 {
        return Err("Weather API returned no daily records for that date range.".to_owned());
    }

    let maxs = expand_f64_vec(daily.temperature_2m_max, len);
    let mins = expand_f64_vec(daily.temperature_2m_min, len);
    let precips = expand_f64_vec(daily.precipitation_sum, len);
    let winds = expand_f64_vec(daily.wind_speed_10m_max, len);
    let codes = expand_i32_vec(daily.weather_code, len);

    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let date_text = &daily.time[i];

        // Validate day values so the app fails with a friendly message if API shape changes.
        date_utils::parse_date(date_text)?;

        let code = codes[i];
        let description = code
            .map(|c| weather_code_map::map_weather_code(c).0.to_owned())
            .unwrap_or_else(|| "Unavailable".to_owned());

        out.push(WeatherDayRecord {
            day_date: date_text.clone(),
            temperature_max_c: maxs[i],
            temperature_min_c: mins[i],
            precipitation_mm: precips[i],
            wind_speed_max_kmh: winds[i],
            weather_code: code,
            weather_description: description,
            source: source.to_owned(),
        });
    }

    Ok(out)
}

fn expand_f64_vec(values: Option<Vec<f64>>, len: usize) -> Vec<Option<f64>> {
    match values {
        Some(v) if v.len() == len => v.into_iter().map(Some).collect(),
        _ => vec![None; len],
    }
}

fn expand_i32_vec(values: Option<Vec<i32>>, len: usize) -> Vec<Option<i32>> {
    match values {
        Some(v) if v.len() == len => v.into_iter().map(Some).collect(),
        _ => vec![None; len],
    }
}
