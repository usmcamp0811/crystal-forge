# Derivation Status Breakdown View

## Overview

The `view_derivation_status_breakdown` provides comprehensive statistics about the distribution of derivation build statuses across the entire system. This view aggregates derivation data to show patterns in build success rates, failure types, and system health trends.

## Status Analysis

The view groups all derivations by their status and provides detailed metrics for each category:

- **Build distribution**: Total counts and percentages of each status type
- **Type breakdown**: Separate counts for `nixos` vs `package` derivations
- **Performance metrics**: Average attempt counts and build durations
- **Temporal analysis**: Recent activity patterns and scheduling trends
- **System health indicators**: Success rates and failure patterns

## Key Fields

| Field                  | Description                                          |
| ---------------------- | ---------------------------------------------------- |
| `status_name`          | Derivation status (complete, failed, pending, etc.)  |
| `status_description`   | Human-readable status explanation                    |
| `is_terminal`          | Whether this status represents a final state         |
| `is_success`           | Whether this status indicates successful completion  |
| `total_count`          | Total derivations with this status                   |
| `nixos_count`          | Count of system-level derivations                    |
| `package_count`        | Count of package-level derivations                   |
| `avg_attempts`         | Average number of build attempts per derivation      |
| `avg_duration_seconds` | Average evaluation time in seconds                   |
| `oldest_scheduled`     | Earliest scheduled derivation with this status       |
| `newest_scheduled`     | Most recent scheduled derivation with this status    |
| `count_last_24h`       | Derivations scheduled in the last 24 hours           |
| `percentage_of_total`  | Percentage this status represents of all derivations |

## Status Categories

**Terminal Success States**:

- `complete`: Successfully built derivations
- `build-complete`: Alternative success status

**Terminal Failure States**:

- `failed`: Build failures due to errors
- `eval-failed`: Evaluation/parsing failures
- `timeout`: Builds that exceeded time limits

**Non-Terminal States**:

- `pending`: Queued for building
- `scheduled`: Waiting to start
- `building`: Currently in progress

## Metrics Interpretation

**Success Indicators**:

- High percentage of `complete` status
- Low average attempt counts (< 2.0 indicates reliable builds)
- Short average durations (efficient evaluation)

**Health Warning Signs**:

- High percentage of `failed` or timeout statuses
- High average attempt counts (indicates build instability)
- Large numbers of long-running `pending` builds
- Skewed `count_last_24h` suggesting recent issues

## Operational Insights

**Build System Performance**: The `avg_duration_seconds` and `avg_attempts` metrics indicate build system efficiency and reliability. Increasing durations or attempt counts may signal infrastructure issues.

**Failure Pattern Analysis**: Distribution between `failed`, `eval-failed`, and `timeout` statuses helps identify whether issues are code-related, infrastructure-related, or resource-related.

**Capacity Planning**: High `pending` or `scheduled` counts relative to recent activity (`count_last_24h`) may indicate build queue backlogs requiring capacity increases.

**Quality Trends**: Tracking `percentage_of_total` over time for success vs failure statuses provides build quality trend analysis.

## Use Cases

This view supports:

- **Build System Monitoring**: Track overall build health and identify systemic issues
- **Performance Analysis**: Monitor build efficiency and resource utilization trends
- **Quality Assurance**: Identify patterns in build failures and success rates
- **Capacity Planning**: Understand build queue depth and processing rates
- **Infrastructure Troubleshooting**: Correlate build issues with system changes
- **Developer Experience**: Assess build reliability from developer perspective

## Query Examples

```sql
-- Overall build health summary
SELECT
    status_name,
    total_count,
    percentage_of_total,
    avg_attempts
FROM view_derivation_status_breakdown
WHERE total_count > 0
ORDER BY percentage_of_total DESC;

-- Build failure analysis
SELECT
    status_name,
    total_count,
    nixos_count,
    package_count,
    avg_duration_seconds
FROM view_derivation_status_breakdown
WHERE is_success = false AND is_terminal = true
ORDER BY total_count DESC;

-- Recent build activity
SELECT
    status_name,
    count_last_24h,
    total_count,
    ROUND((count_last_24h::numeric / total_count) * 100, 1) as recent_activity_pct
FROM view_derivation_status_breakdown
WHERE total_count > 0
ORDER BY recent_activity_pct DESC;

-- Build queue analysis
SELECT
    status_name,
    total_count,
    oldest_scheduled,
    newest_scheduled,
    avg_duration_seconds
FROM view_derivation_status_breakdown
WHERE is_terminal = false
ORDER BY total_count DESC;

-- Success rate calculation
SELECT
    SUM(CASE WHEN is_success = true THEN total_count ELSE 0 END) as successful_builds,
    SUM(CASE WHEN is_terminal = true THEN total_count ELSE 0 END) as completed_builds,
    ROUND(
        (SUM(CASE WHEN is_success = true THEN total_count ELSE 0 END)::numeric /
         SUM(CASE WHEN is_terminal = true THEN total_count ELSE 0 END)) * 100,
        2
    ) as success_rate_pct
FROM view_derivation_status_breakdown
WHERE is_terminal = true;
```

## Notes

- Results are ordered by `display_order` from the derivation_statuses table for consistent presentation
- Percentages are calculated against all derivations system-wide, providing fleet-level perspective
- The view includes both active builds (non-terminal) and historical builds (terminal) for comprehensive analysis
- Time-based metrics use `scheduled_at` timestamps to track build queue patterns
- Average calculations exclude NULL values to provide accurate metrics where data is available
