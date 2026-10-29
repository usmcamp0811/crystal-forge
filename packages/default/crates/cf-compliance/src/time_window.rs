//! Validates the shared fields of a deployment time window.
//!
//! This is the database-free half of the time-window policy: the parsing and
//! validation rules that policy import, export, and generation must share with
//! the server's evaluator. Evaluation (`is_deployment_allowed`) stays in
//! `cf-server`, which re-exports these items unchanged.

use chrono::NaiveTime;
use chrono_tz::Tz;

/// Parses a deployment-window clock value as hours and minutes.
///
/// # Errors
/// Returns an error unless the input has two colon-separated unsigned numbers
/// whose values form a valid hour and minute. Seconds are always zero.
pub fn parse_time(time_str: &str) -> Result<NaiveTime, String> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid time format '{}', expected HH:MM",
            time_str
        ));
    }

    let hour: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid hour in '{}'", time_str))?;
    let minute: u32 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid minute in '{}'", time_str))?;

    NaiveTime::from_hms_opt(hour, minute, 0)
        .ok_or_else(|| format!("Hour {} or minute {} out of range", hour, minute))
}

/// Validates the shared fields of a deployment time window.
///
/// # Errors
///
/// Returns an error when no valid weekday is supplied, a clock value is not in
/// `HH:MM` form, or `timezone` is not a valid IANA time-zone identifier.
pub fn validate_window_parts(
    days: &[String],
    start_time: &str,
    end_time: &str,
    timezone: &str,
) -> Result<(), String> {
    const WEEKDAYS: &[&str] = &["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

    if days.is_empty()
        || days
            .iter()
            .any(|day| !WEEKDAYS.contains(&day.to_ascii_lowercase().as_str()))
    {
        return Err("days must contain only mon, tue, wed, thu, fri, sat, or sun".to_string());
    }
    parse_time(start_time).map_err(|error| format!("invalid start time: {error}"))?;
    parse_time(end_time).map_err(|error| format!("invalid end time: {error}"))?;
    timezone
        .parse::<Tz>()
        .map_err(|_| format!("invalid IANA timezone: {timezone}"))?;
    Ok(())
}
