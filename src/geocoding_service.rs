//! Open-Meteo geocoding service helpers.
//!
//! This module resolves a city/state pair into coordinates before a weather query.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GeocodingResponse {
    results: Option<Vec<GeocodingResult>>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResult {
    name: String,
    admin1: Option<String>,
    country_code: Option<String>,
    latitude: f64,
    longitude: f64,
    timezone: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedLocation {
    pub city: String,
    pub state: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: Option<String>,
}

/// Resolves a US city/state query to the best matching Open-Meteo geocoding result.
pub fn geocode_city_state(city: &str, state: &str) -> Result<ResolvedLocation, String> {
    let trimmed_city = city.trim();
    let trimmed_state = state.trim();

    if trimmed_city.is_empty() || trimmed_state.is_empty() {
        return Err("City and state are required.".to_owned());
    }

    let client = reqwest::blocking::Client::new();
    let response = client
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[
            ("name", trimmed_city),
            ("count", "20"),
            ("language", "en"),
            ("format", "json"),
            ("countryCode", "US"),
        ])
        .send()
        .map_err(|e| format!("Geocoding request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("Geocoding API returned error: {e}"))?;

    let payload: GeocodingResponse = response
        .json()
        .map_err(|e| format!("Failed to parse geocoding JSON: {e}"))?;

    let results = payload
        .results
        .ok_or_else(|| "No matching city results from geocoding API.".to_owned())?;

    // We prioritize exact state abbreviation/name matches first.
    let state_upper = trimmed_state.to_uppercase();
    let best = results
        .iter()
        .find(|r| {
            if r.country_code.as_deref() != Some("US") {
                return false;
            }
            if let Some(admin1) = &r.admin1 {
                let admin_upper = admin1.to_uppercase();
                admin_upper == state_upper || admin_upper.contains(&state_upper)
            } else {
                false
            }
        })
        .or_else(|| results.iter().find(|r| r.country_code.as_deref() == Some("US")))
        .ok_or_else(|| format!("No matching US city found for {trimmed_city}, {trimmed_state}."))?;

    Ok(ResolvedLocation {
        city: best.name.clone(),
        state: best.admin1.clone().unwrap_or_else(|| trimmed_state.to_owned()),
        latitude: best.latitude,
        longitude: best.longitude,
        timezone: best.timezone.clone(),
    })
}
