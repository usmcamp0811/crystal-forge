//! Assertion helpers for Crystal Forge tests.
//!
//! These wrappers surface better panic messages and reduce boilerplate in
//! tests that repeatedly check the same patterns.

use serde::Serialize;

/// Assert that `value` serialises to JSON and the resulting object contains
/// the expected `key` with the expected `expected` value.
///
/// Panics with a descriptive message showing the full JSON on failure.
pub fn assert_json_field<T: Serialize>(value: &T, key: &str, expected: &serde_json::Value) {
    let json = serde_json::to_value(value).expect("failed to serialise value to JSON");
    let obj = json
        .as_object()
        .expect("serialised value is not a JSON object");
    let actual = obj.get(key);
    assert_eq!(
        actual,
        Some(expected),
        "JSON field \"{key}\" mismatch.\n  expected: {expected}\n  actual:   {actual:?}\n  full JSON: {json}",
    );
}

/// Assert that `value` serialises to a JSON object that does **not** contain `key`.
pub fn assert_json_field_absent<T: Serialize>(value: &T, key: &str) {
    let json = serde_json::to_value(value).expect("failed to serialise value to JSON");
    let obj = json
        .as_object()
        .expect("serialised value is not a JSON object");
    assert!(
        !obj.contains_key(key),
        "Expected JSON field \"{key}\" to be absent, but found: {:?}\n  full JSON: {json}",
        obj.get(key),
    );
}

/// Assert that two `DateTime<Utc>` values are within `tolerance` of each other.
///
/// Useful when comparing timestamps that may differ by a few milliseconds
/// due to builder construction vs. test assertion timing.
pub fn assert_timestamps_close(
    a: chrono::DateTime<chrono::Utc>,
    b: chrono::DateTime<chrono::Utc>,
    tolerance: chrono::Duration,
) {
    let diff = (a - b).abs();
    assert!(
        diff <= tolerance,
        "Timestamps differ by {diff} which exceeds tolerance of {tolerance}.\n  a = {a}\n  b = {b}",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    #[derive(serde::Serialize)]
    struct Sample {
        name: String,
        count: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        optional: Option<String>,
    }

    #[test]
    fn assert_json_field_passes_on_match() {
        let s = Sample {
            name: "test".into(),
            count: 42,
            optional: None,
        };
        assert_json_field(&s, "name", &json!("test"));
        assert_json_field(&s, "count", &json!(42));
    }

    #[test]
    #[should_panic(expected = "JSON field \"count\" mismatch")]
    fn assert_json_field_panics_on_mismatch() {
        let s = Sample {
            name: "test".into(),
            count: 42,
            optional: None,
        };
        assert_json_field(&s, "count", &json!(99));
    }

    #[test]
    fn assert_json_field_absent_passes_when_missing() {
        let s = Sample {
            name: "test".into(),
            count: 1,
            optional: None,
        };
        assert_json_field_absent(&s, "optional");
    }

    #[test]
    #[should_panic(expected = "Expected JSON field \"name\" to be absent")]
    fn assert_json_field_absent_panics_when_present() {
        let s = Sample {
            name: "test".into(),
            count: 1,
            optional: None,
        };
        assert_json_field_absent(&s, "name");
    }

    #[test]
    fn assert_timestamps_close_passes_within_tolerance() {
        let now = Utc::now();
        let later = now + chrono::Duration::milliseconds(50);
        assert_timestamps_close(now, later, chrono::Duration::seconds(1));
    }

    #[test]
    #[should_panic(expected = "Timestamps differ by")]
    fn assert_timestamps_close_panics_outside_tolerance() {
        let now = Utc::now();
        let much_later = now + chrono::Duration::hours(1);
        assert_timestamps_close(now, much_later, chrono::Duration::seconds(1));
    }
}
