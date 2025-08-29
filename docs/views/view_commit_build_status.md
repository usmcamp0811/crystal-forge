# Commit Build Status View

## Overview

The `view_commit_build_status` provides detailed visibility into the build status of all commits and their associated derivations. This view enables filtering and analysis of build progress across the entire commit history, making it easy to assess build health and identify problematic commits.

## Build Status Categories

Commits are classified based on their derivation build outcomes:

- **`complete`**: All derivations built successfully
- **`failed`**: All derivations failed to build
- **`partial`**: Some derivations succeeded, others failed
- **`building`**: One or more derivations still in progress
- **`no_builds`**: No derivations scheduled for this commit
- **`unknown`**: Build status cannot be determined

## Key Information Levels

The view provides information at two levels:

### Individual Derivation Level

- Specific derivation details (name, type, status, attempts, duration)
- Build status and error messages
- Evaluation metrics and timing

### Commit Level Aggregations

- Total, successful, failed, and in-progress derivation counts
- Overall commit build status assessment
- Maximum attempts and average durations across all derivations
- Summary of all derivation statuses

## Important Fields

| Field                      | Description                                     |
| -------------------------- | ----------------------------------------------- |
| `git_commit_hash`          | Full Git commit identifier                      |
| `short_hash`               | 8-character commit identifier for display       |
| `commit_timestamp`         | When the commit was made                        |
| `commit_build_status`      | Overall build assessment for the commit         |
| `total_derivations`        | Number of derivations scheduled for this commit |
| `successful_derivations`   | Count of successfully built derivations         |
| `failed_derivations`       | Count of failed derivations                     |
| `in_progress_derivations`  | Count of derivations still building             |
| `derivation_status`        | Individual derivation build status              |
| `derivation_attempt_count` | Number of build attempts for this derivation    |
| `all_statuses`             | Comma-separated list of all derivation statuses |
| `commit_build_description` | Human-readable commit build summary             |

## Data Organization

Results are ordered by:

1. **Commit timestamp** (descending) - newest commits first
2. **Flake name** - grouped by repository
3. **Derivation type** - nixos systems before packages
4. **Derivation name** - alphabetical within type

## Primary Use Cases

**Build Health Monitoring**: Quickly identify commits with build failures or mixed results that need attention.

**Commit-Specific Analysis**: Filter to a specific commit hash to see all its derivation build statuses and progress.

**Build Trend Analysis**: Track build success rates over time by examining recent commits.

**Failure Investigation**: Identify patterns in build failures across commits and derivation types.

**Release Readiness**: Assess whether commits are ready for deployment based on build completion.

## Query Examples

```sql
-- Show all builds for a specific commit
SELECT derivation_name, derivation_type, derivation_status,
       derivation_attempt_count, error_message
FROM view_commit_build_status
WHERE git_commit_hash = 'abc123def456789'
ORDER BY derivation_type, derivation_name;

-- Recent commits with build issues
SELECT short_hash, commit_timestamp, commit_build_status,
       total_derivations, successful_derivations, failed_derivations
FROM view_commit_build_status
WHERE commit_build_status IN ('failed', 'partial')
  AND commit_timestamp > NOW() - INTERVAL '7 days'
GROUP BY commit_id, short_hash, commit_timestamp, commit_build_status,
         total_derivations, successful_derivations, failed_derivations
ORDER BY commit_timestamp DESC;

-- Build success rate by flake over last 30 days
SELECT flake_name,
       COUNT(DISTINCT commit_id) as total_commits,
       COUNT(DISTINCT CASE WHEN commit_build_status = 'complete' THEN commit_id END) as successful_commits,
       ROUND(
         COUNT(DISTINCT CASE WHEN commit_build_status = 'complete' THEN commit_id END) * 100.0 /
         COUNT(DISTINCT commit_id), 2
       ) as success_rate_percent
FROM view_commit_build_status
WHERE commit_timestamp > NOW() - INTERVAL '30 days'
GROUP BY flake_name
ORDER BY success_rate_percent DESC;

-- Commits currently building
SELECT short_hash, flake_name, in_progress_derivations,
       total_derivations, commit_build_description
FROM view_commit_build_status
WHERE commit_build_status = 'building'
GROUP BY commit_id, short_hash, flake_name, in_progress_derivations,
         total_derivations, commit_build_description
ORDER BY commit_timestamp DESC;
```

## Integration Notes

This view complements other system views by focusing specifically on build status rather than deployment status. While `view_system_deployment_status` shows what systems are running, this view shows what commits are available to deploy based on build success.

The view's dual-level design allows you to either get commit-level summaries (using `GROUP BY commit_id`) or drill down into individual derivation details for troubleshooting specific build issues.
