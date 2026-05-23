// Time window policy evaluation service
// Checks if current time falls within configured deployment windows

use chrono::{Datelike, NaiveTime, Timelike, Utc};
use chrono_tz::Tz;

use crate::models::deployment_policies::TimeWindowConfig;

/// Result of time window policy evaluation
#[derive(Debug, Clone)]
pub struct TimeWindowResult {
    pub deployment_allowed: bool,
    pub reason: Option<String>,
}

/// Evaluate a time window policy against current time
pub fn check_time_window(config: &TimeWindowConfig) -> TimeWindowResult {
    let now_utc = Utc::now();
    
    // Normalize action to lowercase for case-insensitive comparison
    let action = config.action.to_lowercase();
    
    // Parse timezone
    let tz: Tz = match config.timezone.parse() {
        Ok(tz) => tz,
        Err(_) => {
            return TimeWindowResult {
                deployment_allowed: false,
                reason: Some(format!("Invalid timezone: {}", config.timezone)),
            };
        }
    };
    
    let now_local = now_utc.with_timezone(&tz);
    
    // Check day of week
    let current_day = match now_local.weekday() {
        chrono::Weekday::Mon => "mon",
        chrono::Weekday::Tue => "tue",
        chrono::Weekday::Wed => "wed",
        chrono::Weekday::Thu => "thu",
        chrono::Weekday::Fri => "fri",
        chrono::Weekday::Sat => "sat",
        chrono::Weekday::Sun => "sun",
    };
    
    let day_allowed = config.days.iter().any(|d| d.to_lowercase() == current_day);
    
    if !day_allowed {
        let reason = format!(
            "Current day {} not in allowed days: {}",
            current_day,
            config.days.join(", ")
        );
        return TimeWindowResult {
            deployment_allowed: action == "warn",
            reason: Some(reason),
        };
    }
    
    // Parse time range
    let start_time = match parse_time(&config.start_time) {
        Ok(t) => t,
        Err(e) => {
            return TimeWindowResult {
                deployment_allowed: false,
                reason: Some(format!("Invalid start_time: {}", e)),
            };
        }
    };
    
    let end_time = match parse_time(&config.end_time) {
        Ok(t) => t,
        Err(e) => {
            return TimeWindowResult {
                deployment_allowed: false,
                reason: Some(format!("Invalid end_time: {}", e)),
            };
        }
    };
    
    let current_time = NaiveTime::from_hms_opt(
        now_local.hour(),
        now_local.minute(),
        now_local.second(),
    )
    .unwrap();
    
    let time_in_range = if start_time <= end_time {
        // Normal range: 09:00 - 17:00
        current_time >= start_time && current_time <= end_time
    } else {
        // Wrap-around range: 22:00 - 02:00 (crosses midnight)
        current_time >= start_time || current_time <= end_time
    };
    
    if !time_in_range {
        let reason = format!(
            "Current time {:02}:{:02} not in window {}-{} ({} timezone)",
            now_local.hour(),
            now_local.minute(),
            config.start_time,
            config.end_time,
            config.timezone
        );
        return TimeWindowResult {
            deployment_allowed: action == "warn",
            reason: Some(reason),
        };
    }
    
    TimeWindowResult {
        deployment_allowed: true,
        reason: None,
    }
}

/// Parse time string in HH:MM format
fn parse_time(time_str: &str) -> Result<NaiveTime, String> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid time format '{}', expected HH:MM", time_str));
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_time_valid() {
        let time = parse_time("09:00").unwrap();
        assert_eq!(time.hour(), 9);
        assert_eq!(time.minute(), 0);
    }
    
    #[test]
    fn test_parse_time_invalid_format() {
        assert!(parse_time("9:00:00").is_err());
        assert!(parse_time("9").is_err());
        assert!(parse_time("not a time").is_err());
    }
    
    #[test]
    fn test_parse_time_out_of_range() {
        assert!(parse_time("25:00").is_err());
        assert!(parse_time("12:60").is_err());
    }
}
