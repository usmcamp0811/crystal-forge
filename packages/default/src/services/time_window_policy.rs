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
    check_time_window_at(config, Utc::now())
}

/// Evaluate a time window policy against a specific timestamp (testable)
pub fn check_time_window_at(
    config: &TimeWindowConfig,
    now_utc: chrono::DateTime<Utc>,
) -> TimeWindowResult {
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
    
    // Helper to get short weekday name
    let weekday_name = |wd: chrono::Weekday| -> &str {
        match wd {
            chrono::Weekday::Mon => "mon",
            chrono::Weekday::Tue => "tue",
            chrono::Weekday::Wed => "wed",
            chrono::Weekday::Thu => "thu",
            chrono::Weekday::Fri => "fri",
            chrono::Weekday::Sat => "sat",
            chrono::Weekday::Sun => "sun",
        }
    };
    
    let is_wrap_around = start_time > end_time;
    
    let (day_allowed, matched_day) = if !is_wrap_around {
        // Normal window (e.g., 09:00 - 17:00): check current day and time
        let current_day = weekday_name(now_local.weekday());
        let day_matches = config.days.iter().any(|d| d.to_lowercase() == current_day);
        let time_in_range = current_time >= start_time && current_time <= end_time;
        (day_matches && time_in_range, current_day)
    } else {
        // Wrap-around window (e.g., 22:00 - 02:00): check both current and previous day
        let current_day = weekday_name(now_local.weekday());
        let previous_day = weekday_name(now_local.weekday().pred());
        
        // If current_time >= start_time, we're in the "late night" part (e.g., Mon 23:00)
        // Check if current day is allowed
        if current_time >= start_time {
            let day_matches = config.days.iter().any(|d| d.to_lowercase() == current_day);
            (day_matches, current_day)
        }
        // If current_time <= end_time, we're in the "early morning" part (e.g., Tue 01:00)
        // Check if previous day is allowed (the window started yesterday)
        else if current_time <= end_time {
            let day_matches = config.days.iter().any(|d| d.to_lowercase() == previous_day);
            (day_matches, previous_day)
        }
        // Outside the wrap-around window entirely
        else {
            (false, current_day)
        }
    };
    
    if !day_allowed {
        let reason = if is_wrap_around {
            format!(
                "Current time {:02}:{:02} on {} not in wrap-around window {}-{} for days: {} ({} timezone)",
                now_local.hour(),
                now_local.minute(),
                weekday_name(now_local.weekday()),
                config.start_time,
                config.end_time,
                config.days.join(", "),
                config.timezone
            )
        } else {
            format!(
                "Current time {:02}:{:02} on {} not in window {}-{} for days: {} ({} timezone)",
                now_local.hour(),
                now_local.minute(),
                matched_day,
                config.start_time,
                config.end_time,
                config.days.join(", "),
                config.timezone
            )
        };
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
    use chrono::TimeZone;
    
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
    
    #[test]
    fn test_normal_window_allowed() {
        let config = TimeWindowConfig {
            description: "Business hours".to_string(),
            days: vec!["mon".to_string(), "tue".to_string(), "wed".to_string()],
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "block".to_string(),
        };
        
        // Monday 2024-01-08 12:00 CST (within window)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 8, 12, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(result.deployment_allowed);
        assert!(result.reason.is_none());
    }
    
    #[test]
    fn test_normal_window_blocked_wrong_day() {
        let config = TimeWindowConfig {
            description: "Weekdays only".to_string(),
            days: vec!["mon".to_string(), "tue".to_string(), "wed".to_string()],
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "block".to_string(),
        };
        
        // Saturday 2024-01-06 12:00 CST (wrong day)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(!result.deployment_allowed);
        assert!(result.reason.is_some());
    }
    
    #[test]
    fn test_normal_window_blocked_outside_time() {
        let config = TimeWindowConfig {
            description: "Business hours".to_string(),
            days: vec!["mon".to_string()],
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "block".to_string(),
        };
        
        // Monday 2024-01-08 20:00 CST (outside time range)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 8, 20, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(!result.deployment_allowed);
        assert!(result.reason.is_some());
    }
    
    #[test]
    fn test_overnight_window_monday_late_night_allowed() {
        let config = TimeWindowConfig {
            description: "Monday night maintenance".to_string(),
            days: vec!["mon".to_string()],
            start_time: "22:00".to_string(),
            end_time: "02:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "block".to_string(),
        };
        
        // Monday 2024-01-08 23:00 CST (late night part of window)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 8, 23, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(result.deployment_allowed, "Monday 23:00 should be allowed in Mon 22:00-02:00 window");
        assert!(result.reason.is_none());
    }
    
    #[test]
    fn test_overnight_window_tuesday_early_morning_allowed() {
        let config = TimeWindowConfig {
            description: "Monday night maintenance".to_string(),
            days: vec!["mon".to_string()],
            start_time: "22:00".to_string(),
            end_time: "02:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "block".to_string(),
        };
        
        // Tuesday 2024-01-09 01:00 CST (early morning part of Monday night window)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 9, 1, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(result.deployment_allowed, "Tuesday 01:00 should be allowed as part of Monday night window");
        assert!(result.reason.is_none());
    }
    
    #[test]
    fn test_overnight_window_tuesday_late_morning_blocked() {
        let config = TimeWindowConfig {
            description: "Monday night maintenance".to_string(),
            days: vec!["mon".to_string()],
            start_time: "22:00".to_string(),
            end_time: "02:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "block".to_string(),
        };
        
        // Tuesday 2024-01-09 03:00 CST (outside window)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 9, 3, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(!result.deployment_allowed, "Tuesday 03:00 should be blocked");
        assert!(result.reason.is_some());
    }
    
    #[test]
    fn test_overnight_window_monday_afternoon_blocked() {
        let config = TimeWindowConfig {
            description: "Monday night maintenance".to_string(),
            days: vec!["mon".to_string()],
            start_time: "22:00".to_string(),
            end_time: "02:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "block".to_string(),
        };
        
        // Monday 2024-01-08 21:00 CST (before window starts)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 8, 21, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(!result.deployment_allowed, "Monday 21:00 should be blocked");
        assert!(result.reason.is_some());
    }
    
    #[test]
    fn test_action_warn_allows_with_reason() {
        let config = TimeWindowConfig {
            description: "Preferred hours".to_string(),
            days: vec!["mon".to_string()],
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "warn".to_string(),
        };
        
        // Monday 2024-01-08 20:00 CST (outside window, but action=warn)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 8, 20, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(result.deployment_allowed, "action=warn should allow deployment");
        assert!(result.reason.is_some(), "reason should explain the warning");
    }
    
    #[test]
    fn test_action_warn_overnight_window() {
        let config = TimeWindowConfig {
            description: "Preferred maintenance window".to_string(),
            days: vec!["mon".to_string()],
            start_time: "22:00".to_string(),
            end_time: "02:00".to_string(),
            timezone: "America/Chicago".to_string(),
            action: "warn".to_string(),
        };
        
        // Tuesday 2024-01-09 10:00 CST (outside window, action=warn)
        let chicago_tz: Tz = "America/Chicago".parse().unwrap();
        let timestamp = chicago_tz.with_ymd_and_hms(2024, 1, 9, 10, 0, 0).unwrap();
        let result = check_time_window_at(&config, timestamp.with_timezone(&Utc));
        
        assert!(result.deployment_allowed, "action=warn should allow deployment");
        assert!(result.reason.is_some(), "reason should explain the warning");
    }
}
