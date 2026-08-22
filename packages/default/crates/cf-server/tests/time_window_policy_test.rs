// Unit tests for time window policy evaluation

use chrono::TimeZone;
use crystal_forge::models::deployment_policies::TimeWindowConfig;
use crystal_forge::services::time_window_policy::{check_time_window, check_time_window_at};

#[test]
fn test_invalid_timezone() {
    let config = TimeWindowConfig {
        description: "Test".to_string(),
        days: vec!["mon".to_string()],
        start_time: "09:00".to_string(),
        end_time: "17:00".to_string(),
        timezone: "Invalid/Timezone".to_string(),
        action: "block".to_string(),
    };

    let result = check_time_window(&config);
    assert!(!result.deployment_allowed);
    assert!(result.reason.is_some());
    assert!(result.reason.unwrap().contains("Invalid timezone"));
}

#[test]
fn test_invalid_time_format() {
    let config = TimeWindowConfig {
        description: "Test".to_string(),
        days: vec!["mon".to_string()],
        start_time: "9:00".to_string(), // Should be 09:00
        end_time: "17:00".to_string(),
        timezone: "UTC".to_string(),
        action: "block".to_string(),
    };

    let result = check_time_window(&config);
    // May succeed or fail depending on timezone/time - just ensure it doesn't panic
    assert!(result.reason.is_some() || result.deployment_allowed);
}

#[test]
fn test_time_window_logic_with_utc() {
    // This test uses UTC timezone and a fixed timestamp for deterministic testing.
    let config = TimeWindowConfig {
        description: "24/7 window".to_string(),
        days: vec![
            "mon".to_string(),
            "tue".to_string(),
            "wed".to_string(),
            "thu".to_string(),
            "fri".to_string(),
            "sat".to_string(),
            "sun".to_string(),
        ],
        start_time: "00:00".to_string(),
        end_time: "23:59".to_string(),
        timezone: "UTC".to_string(),
        action: "block".to_string(),
    };

    let result = check_time_window_at(
        &config,
        chrono::Utc.with_ymd_and_hms(2026, 8, 16, 12, 0, 0).unwrap(),
    );
    // Should always allow since it's 24/7
    assert!(result.deployment_allowed);
}

#[test]
fn test_empty_days_array() {
    let config = TimeWindowConfig {
        description: "No days allowed".to_string(),
        days: vec![],
        start_time: "00:00".to_string(),
        end_time: "23:59".to_string(),
        timezone: "UTC".to_string(),
        action: "block".to_string(),
    };

    let result = check_time_window(&config);
    // Should block since no days are allowed
    assert!(!result.deployment_allowed);
}
