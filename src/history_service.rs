//! Persistent history storage and CSV export helpers.

use crate::models::HistoryEntry;
use chrono::Local;
use csv::Writer;
use std::fs;
use std::path::{Path, PathBuf};

/// Flat CSV row so multi-day entries can be exported one day per line.
#[derive(serde::Serialize)]
struct HistoryCsvRow {
    timestamp: String,
    city: String,
    state: String,
    start_date: String,
    end_date: String,
    day_date: String,
    latitude: f64,
    longitude: f64,
    timezone: String,
    source: String,
    current_temperature_c: Option<f64>,
    current_apparent_temperature_c: Option<f64>,
    current_weather_description: Option<String>,
    day_temperature_max_c: Option<f64>,
    day_temperature_min_c: Option<f64>,
    day_precipitation_mm: Option<f64>,
    day_wind_speed_max_kmh: Option<f64>,
    day_weather_code: Option<i32>,
    day_weather_description: String,
}

pub fn history_file_path() -> PathBuf {
    PathBuf::from("weather_history.json")
}

pub fn load_history(path: &Path) -> Result<Vec<HistoryEntry>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let text = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read history file '{}': {e}", path.display()))?;

    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str::<Vec<HistoryEntry>>(&text)
        .map_err(|e| format!("Failed to parse history JSON '{}': {e}", path.display()))
}

pub fn save_history(path: &Path, entries: &[HistoryEntry]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("Failed to serialize history data: {e}"))?;
    fs::write(path, json)
        .map_err(|e| format!("Failed to write history file '{}': {e}", path.display()))
}

pub fn clear_history(path: &Path) -> Result<(), String> {
    save_history(path, &[])
}

pub fn export_history_csv(entries: &[HistoryEntry]) -> Result<PathBuf, String> {
    let filename = format!(
        "weather_history_export_{}.csv",
        Local::now().format("%Y%m%d_%H%M%S")
    );
    let path = PathBuf::from(filename);

    let mut writer = Writer::from_path(&path)
        .map_err(|e| format!("Failed to create CSV '{}': {e}", path.display()))?;

    for entry in entries {
        if entry.days.is_empty() {
            let row = HistoryCsvRow {
                timestamp: entry.timestamp.clone(),
                city: entry.city.clone(),
                state: entry.state.clone(),
                start_date: entry.start_date.clone(),
                end_date: entry.end_date.clone(),
                day_date: String::new(),
                latitude: entry.latitude,
                longitude: entry.longitude,
                timezone: entry.timezone.clone(),
                source: entry.source.clone(),
                current_temperature_c: entry.current_temperature_c,
                current_apparent_temperature_c: entry.current_apparent_temperature_c,
                current_weather_description: entry.current_weather_description.clone(),
                day_temperature_max_c: None,
                day_temperature_min_c: None,
                day_precipitation_mm: None,
                day_wind_speed_max_kmh: None,
                day_weather_code: None,
                day_weather_description: String::new(),
            };
            writer
                .serialize(row)
                .map_err(|e| format!("Failed to write CSV row: {e}"))?;
            continue;
        }

        for day in &entry.days {
            let row = HistoryCsvRow {
                timestamp: entry.timestamp.clone(),
                city: entry.city.clone(),
                state: entry.state.clone(),
                start_date: entry.start_date.clone(),
                end_date: entry.end_date.clone(),
                day_date: day.day_date.clone(),
                latitude: entry.latitude,
                longitude: entry.longitude,
                timezone: entry.timezone.clone(),
                source: day.source.clone(),
                current_temperature_c: entry.current_temperature_c,
                current_apparent_temperature_c: entry.current_apparent_temperature_c,
                current_weather_description: entry.current_weather_description.clone(),
                day_temperature_max_c: day.temperature_max_c,
                day_temperature_min_c: day.temperature_min_c,
                day_precipitation_mm: day.precipitation_mm,
                day_wind_speed_max_kmh: day.wind_speed_max_kmh,
                day_weather_code: day.weather_code,
                day_weather_description: day.weather_description.clone(),
            };
            writer
                .serialize(row)
                .map_err(|e| format!("Failed to write CSV row: {e}"))?;
        }
    }

    writer
        .flush()
        .map_err(|e| format!("Failed to flush CSV '{}': {e}", path.display()))?;

    Ok(path)
}
