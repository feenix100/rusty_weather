//! Shared data models for app state, API payloads, and persistence.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// Location entry for quick-select buttons.
#[derive(Debug, Clone, Copy)]
pub struct QuickCity {
    pub city: &'static str,
    pub state: &'static str,
}

pub const QUICK_CITIES: [QuickCity; 6] = [
    QuickCity {
        city: "Phoenix",
        state: "AZ",
    },
    QuickCity {
        city: "New York",
        state: "NY",
    },
    QuickCity {
        city: "Los Angeles",
        state: "CA",
    },
    QuickCity {
        city: "Chicago",
        state: "IL",
    },
    QuickCity {
        city: "Dallas",
        state: "TX",
    },
    QuickCity {
        city: "Seattle",
        state: "WA",
    },
];

/// Input mode shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    CityState,
    Coordinates,
}

/// Animation behavior for the visualization panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationMode {
    Auto,
    Sunny,
    Rain,
    Snow,
    Cloud,
}

impl AnimationMode {
    pub fn label(self) -> &'static str {
        match self {
            AnimationMode::Auto => "Auto",
            AnimationMode::Sunny => "Sunny",
            AnimationMode::Rain => "Rain",
            AnimationMode::Snow => "Snow",
            AnimationMode::Cloud => "Cloud",
        }
    }

    pub fn all() -> [AnimationMode; 5] {
        [
            AnimationMode::Auto,
            AnimationMode::Sunny,
            AnimationMode::Rain,
            AnimationMode::Snow,
            AnimationMode::Cloud,
        ]
    }
}

/// Parsed user request sent to background worker.
#[derive(Debug, Clone)]
pub enum LocationQuery {
    CityState { city: String, state: String },
    Coordinates { latitude: f64, longitude: f64 },
}

#[derive(Debug, Clone)]
pub struct WeatherQueryRequest {
    pub location: LocationQuery,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
}

/// One day record shown in result cards and used for history/export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherDayRecord {
    pub day_date: String,
    pub temperature_max_c: Option<f64>,
    pub temperature_min_c: Option<f64>,
    pub precipitation_mm: Option<f64>,
    pub wind_speed_max_kmh: Option<f64>,
    pub weather_code: Option<i32>,
    pub weather_description: String,
    pub source: String,
}

/// Full weather response tied to one user query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherQueryResult {
    pub fetched_at: DateTime<Local>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
    pub start_date: String,
    pub end_date: String,
    pub current_temperature_c: Option<f64>,
    pub current_apparent_temperature_c: Option<f64>,
    pub current_weather_code: Option<i32>,
    pub current_weather_description: Option<String>,
    pub days: Vec<WeatherDayRecord>,
}

/// Persisted history entry. We store a snapshot of each successful query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: String,
    pub city: String,
    pub state: String,
    pub start_date: String,
    pub end_date: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
    pub source: String,
    pub current_temperature_c: Option<f64>,
    pub current_apparent_temperature_c: Option<f64>,
    pub current_weather_description: Option<String>,
    pub days: Vec<WeatherDayRecord>,
}

impl HistoryEntry {
    pub fn from_result(result: &WeatherQueryResult) -> Self {
        let source_summary = if result.days.is_empty() {
            "unknown".to_owned()
        } else {
            summarize_sources(&result.days)
        };

        HistoryEntry {
            timestamp: result.fetched_at.to_rfc3339(),
            city: result.city.clone().unwrap_or_else(|| "(coords)".to_owned()),
            state: result.state.clone().unwrap_or_else(|| "-".to_owned()),
            start_date: result.start_date.clone(),
            end_date: result.end_date.clone(),
            latitude: result.latitude,
            longitude: result.longitude,
            timezone: result.timezone.clone(),
            source: source_summary,
            current_temperature_c: result.current_temperature_c,
            current_apparent_temperature_c: result.current_apparent_temperature_c,
            current_weather_description: result.current_weather_description.clone(),
            days: result.days.clone(),
        }
    }
}

fn summarize_sources(days: &[WeatherDayRecord]) -> String {
    let mut has_forecast = false;
    let mut has_archive = false;
    for day in days {
        if day.source == "forecast" {
            has_forecast = true;
        }
        if day.source == "archive" {
            has_archive = true;
        }
    }
    match (has_archive, has_forecast) {
        (true, true) => "archive+forecast".to_owned(),
        (true, false) => "archive".to_owned(),
        (false, true) => "forecast".to_owned(),
        (false, false) => "unknown".to_owned(),
    }
}
