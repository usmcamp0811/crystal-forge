# Frontend Work Remaining for TASK-215

The backend infrastructure for fast flakes view loading is complete. The following frontend work remains:

## Phase 5: Fix Evaluation Status Chip

**File**: `packages/web-ui/src/views/flakes.rs` or wherever evaluation status is rendered

**Current Issue**: Shows "❌ Evaluation Error" for policy failures when it should show "⚠️ Partial"

**Fix Needed**:
```rust
fn render_evaluation_status(commit: &FlakeCommit) -> Element {
    match commit.evaluation_status.as_deref() {
        Some("complete") => {
            if let Some(meta) = &commit.metadata {
                if meta.all_systems_passed {
                    rsx! { Chip { color: Color::Success, "✅ Complete ({meta.systems_passed_policy}/{meta.total_systems})" } }
                } else if meta.has_nix_eval_error {
                    rsx! { Chip { color: Color::Danger, "❌ Evaluation Error" } }
                } else {
                    // Policy failures - NOT an error
                    rsx! { Chip { color: Color::Warning, "⚠️ Partial ({meta.systems_passed_policy}/{meta.total_systems})" } }
                }
            } else {
                // Fallback when cache not yet populated
                rsx! { Chip { color: Color::Success, "✅ Complete" } }
            }
        }
        Some("failed") => rsx! { Chip { color: Color::Danger, "❌ Failed" } },
        Some("in_progress") => rsx! { Chip { color: Color::Info, "⏳ Evaluating" } },
        _ => rsx! { Chip { color: Color::Base, "⏸️ Pending" } },
    }
}
```

## Phase 6: Fix System Status Chip Theming

**Files**: Wherever system status chips are rendered

**Current Issue**: Inconsistent colors, not using design system correctly

**Fix Needed**:
```rust
fn render_system_status_chip(system: &System) -> Element {
    let (color, icon, label) = match system.status.as_str() {
        "queued_for_build" => (Color::Info, "⏳", "Queued"),
        "building" => (Color::Info, "🔨", "Building"),
        "build_complete" => (Color::Success, "✅", "Built"),
        "build_failed" => (Color::Danger, "❌", "Build Failed"),
        "deployed" => (Color::Success, "🚀", "Deployed"),
        "policy_failed" => (Color::Warning, "⚠️", "Policy Failed"),
        _ => (Color::Base, "❓", "Unknown"),
    };
    
    rsx! { Chip { color: color, "{icon} {label}" } }
}
```

Verify `Chip` component applies color prop correctly to theme CSS classes.

## Phase 7: Browser Timezone Display

**File**: Create `packages/web-ui/src/components/timestamp.rs`

**Implementation**:
```rust
use chrono::{DateTime, Utc, Local};
use dioxus::prelude::*;

#[component]
pub fn Timestamp(
    datetime: DateTime<Utc>,
    #[props(default = "relative")] format: &'static str,
) -> Element {
    let local_time = datetime.with_timezone(&Local);
    
    let formatted = match format {
        "relative" => format_relative(&local_time),
        "short" => local_time.format("%Y-%m-%d %H:%M").to_string(),
        "long" => local_time.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
        _ => local_time.to_rfc3339(),
    };
    
    rsx! {
        span {
            title: "{local_time.format(\"%Y-%m-%d %H:%M:%S %Z\")}",
            "{formatted}"
        }
    }
}

fn format_relative(dt: &DateTime<Local>) -> String {
    let now = Local::now();
    let duration = now.signed_duration_since(*dt);
    
    if duration.num_seconds() < 60 { "just now".to_string() }
    else if duration.num_minutes() < 60 { format!("{} min ago", duration.num_minutes()) }
    else if duration.num_hours() < 24 { format!("{} hours ago", duration.num_hours()) }
    else if duration.num_days() < 7 { format!("{} days ago", duration.num_days()) }
    else { dt.format("%Y-%m-%d").to_string() }
}
```

**Usage**: Replace all raw timestamp displays with `<Timestamp datetime={commit.committed_at} />`

## Testing After Frontend Changes

1. Load flakes view - should load in <2 seconds
2. Check evaluation status chips show correct labels
3. Check system status chips have correct colors
4. Check timestamps show in local timezone
5. Verify tooltips work on chips

## Notes

- Backend is complete and functional
- Cache will start populating immediately after migration
- Frontend can be updated incrementally without breaking existing behavior
- metadata field is optional, so missing cache entries gracefully fall back
