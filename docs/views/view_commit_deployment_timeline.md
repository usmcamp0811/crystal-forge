# Commit Deployment Timeline View

## Overview

The `view_commit_deployment_timeline` provides a commit-centric view of your deployment pipeline, tracking how commits flow from evaluation through deployment across your fleet. This view focuses on the temporal aspects of deployments, showing when commits were evaluated, when systems first adopted them, and which systems are currently running each commit.

## Time Scope

The view focuses on **recent activity** by filtering to commits from the last 30 days. This keeps the view performant and relevant for operational decision-making while providing sufficient history for trend analysis.

## Key Concepts

### Deployment Approximation

Since there's no explicit "deployment event" table, the view approximates deployment timing by tracking when systems first appear in `system_states` after a commit timestamp. This "first seen after commit" approach provides a practical proxy for deployment timing.

### Evaluation vs Deployment

The view distinguishes between:

- **Evaluation**: Whether derivations for a commit built successfully
- **Deployment**: Whether systems actually adopted and ran the commit

## Important Fields

| Field                             | Description                                          |
| --------------------------------- | ---------------------------------------------------- |
| `git_commit_hash`                 | Full commit identifier                               |
| `short_hash`                      | 8-character commit identifier for display            |
| `commit_timestamp`                | When the commit was made                             |
| `total_evaluations`               | Number of derivations attempted for this commit      |
| `successful_evaluations`          | Number of derivations that built successfully        |
| `evaluation_statuses`             | Comma-separated list of derivation statuses          |
| `evaluated_targets`               | Systems/packages that were evaluated for this commit |
| `first_deployment`                | When the first system adopted this commit            |
| `last_deployment`                 | When the last system adopted this commit             |
| `total_systems_deployed`          | Number of systems that have ever run this commit     |
| `currently_deployed_systems`      | Number of systems currently running this commit      |
| `deployed_systems`                | Names of systems that have deployed this commit      |
| `currently_deployed_systems_list` | Names of systems currently running this commit       |

## Data Ordering

Results are ordered by `commit_timestamp DESC`, showing the most recent commits first. This ordering supports operational workflows where recent activity is most relevant.

## Use Cases

**Deployment Velocity Tracking**: Monitor how quickly commits flow from creation to deployment across your infrastructure.

**Rollout Analysis**: Track deployment rollouts to see adoption patterns and identify systems that may be lagging behind.

**Evaluation Success Monitoring**: Identify commits with build failures that are blocking deployment progress.

**Fleet Status Assessment**: See which commits are currently deployed where, enabling targeted rollbacks or updates.

**Deployment Timeline Analysis**: Understand the time gap between commit creation and fleet-wide adoption.

## Operational Workflows

**Release Management**: Track rollout progress of new releases by monitoring deployment spread across systems.

**Incident Response**: Quickly identify which systems are running problematic commits and plan targeted rollbacks.

**Build Pipeline Health**: Monitor evaluation success rates and identify commits causing build failures.

**Compliance Reporting**: Generate deployment timeline reports for audit and compliance purposes.

## Query Examples

```sql
-- Recent commits with their deployment status
SELECT short_hash, commit_timestamp, total_evaluations, successful_evaluations,
       total_systems_deployed, currently_deployed_systems
FROM view_commit_deployment_timeline
ORDER BY commit_timestamp DESC
LIMIT 10;

-- Commits with deployment issues (built but not deployed)
SELECT short_hash, flake_name, successful_evaluations, total_systems_deployed,
       evaluation_statuses
FROM view_commit_deployment_timeline
WHERE successful_evaluations > 0 AND total_systems_deployed = 0
ORDER BY commit_timestamp DESC;

-- Deployment velocity metrics
SELECT flake_name,
       AVG(EXTRACT(EPOCH FROM (first_deployment - commit_timestamp)) / 3600) as avg_hours_to_first_deployment,
       AVG(EXTRACT(EPOCH FROM (last_deployment - first_deployment)) / 3600) as avg_rollout_duration_hours
FROM view_commit_deployment_timeline
WHERE first_deployment IS NOT NULL
GROUP BY flake_name;

-- Currently active commits across the fleet
SELECT short_hash, flake_name, currently_deployed_systems,
       currently_deployed_systems_list
FROM view_commit_deployment_timeline
WHERE currently_deployed_systems > 0
ORDER BY currently_deployed_systems DESC;

-- Build success rate by flake
SELECT flake_name,
       COUNT(*) as total_commits,
       AVG(CASE WHEN successful_evaluations = total_evaluations THEN 1.0 ELSE 0.0 END) * 100 as build_success_rate
FROM view_commit_deployment_timeline
WHERE total_evaluations > 0
GROUP BY flake_name
ORDER BY build_success_rate DESC;
```

## Integration Notes

This view complements other system views by providing the temporal dimension:

- **System Status Views**: Show current system state
- **Deployment Status Views**: Show what systems should be running
- **Commit Timeline View**: Shows how commits flow through the deployment pipeline over time

The combination provides comprehensive visibility into both current fleet status and deployment pipeline health.

## Limitations

**Deployment Approximation**: The view approximates deployment timing based on system state changes rather than explicit deployment events. This is generally accurate but may not capture the exact moment of deployment.

**30-Day Window**: Only shows recent commits. For historical analysis beyond 30 days, direct table queries may be needed.

**System State Dependency**: Deployment detection relies on systems reporting state changes. Systems that deploy but don't report state changes won't be tracked accurately.
