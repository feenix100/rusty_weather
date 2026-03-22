//! Date parsing and formatting helpers used across UI validation and API calls.

use chrono::{Local, NaiveDate};

/// Parses a YYYY-MM-DD date string into `NaiveDate`.
///
/// Keeping this in one place ensures all modules use the same validation rule.
pub fn parse_date(input: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d")
        .map_err(|_| format!("Invalid date '{input}'. Use YYYY-MM-DD."))
}

/// Returns today's local date in YYYY-MM-DD format.
pub fn today_string() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}
