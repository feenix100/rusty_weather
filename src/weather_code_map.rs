//! Weather code mapping from Open-Meteo numeric codes into user-facing labels
//! and animation modes.

use crate::models::AnimationMode;

/// Converts an Open-Meteo weather code into:
/// 1) A readable weather description.
/// 2) A default animation mode used when UI is in Auto mode.
pub fn map_weather_code(code: i32) -> (&'static str, AnimationMode) {
    match code {
        0 => ("Clear sky", AnimationMode::Sunny),
        1 | 2 => ("Mainly clear", AnimationMode::Cloud),
        3 => ("Overcast", AnimationMode::Cloud),
        45 | 48 => ("Fog", AnimationMode::Cloud),
        51 | 53 | 55 => ("Drizzle", AnimationMode::Rain),
        56 | 57 => ("Freezing drizzle", AnimationMode::Snow),
        61 | 63 | 65 => ("Rain", AnimationMode::Rain),
        66 | 67 => ("Freezing rain", AnimationMode::Snow),
        71 | 73 | 75 | 77 => ("Snow", AnimationMode::Snow),
        80 | 81 | 82 => ("Rain showers", AnimationMode::Rain),
        85 | 86 => ("Snow showers", AnimationMode::Snow),
        95 => ("Thunderstorm", AnimationMode::Rain),
        96 | 99 => ("Thunderstorm with hail", AnimationMode::Rain),
        _ => ("Unknown conditions", AnimationMode::Cloud),
    }
}
