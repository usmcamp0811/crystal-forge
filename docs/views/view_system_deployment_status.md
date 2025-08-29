# System Deployment Status View

## Overview

The `view_system_deployment_status` tracks where each system stands relative to the latest commit in their associated flake repository. This view answers the critical question: "Is this system running the latest available code?"

## Status Categories

Systems are classified into deployment status categories based on their relationship to their flake's commit history:

- **`up_to_date`**: System is running the latest commit from its flake
- **`behind`**: System is running an older commit with newer commits available
- **`no_deployment`**: System is registered but has never been deployed
- **`unknown`**: System has a deployment but the derivation can't be related to any flake
- **`ahead`**: System is running a newer commit than expected (rare edge case)

## Key Relationships

The view traces the deployment chain:

1. `system_states.derivation_path` → identifies what's currently deployed
2. `derivations` table → links deployment to commit via derivation_path
3. `commits` table → provides commit timeline and flake association
4. `flakes` table → groups commits by repository

## Important Fields

| Field                      | Description                                  |
| -------------------------- | -------------------------------------------- |
| `hostname`                 | System identifier                            |
| `deployment_status`        | Current deployment status                    |
| `commits_behind`           | Number of commits between current and latest |
| `current_commit_hash`      | Git hash of currently deployed commit        |
| `latest_commit_hash`       | Git hash of latest available commit          |
| `current_commit_timestamp` | When current commit was made                 |
| `latest_commit_timestamp`  | When latest commit was made                  |
| `deployment_time`          | When current deployment occurred             |
| `flake_name`               | Associated flake/repository name             |
| `status_description`       | Human-readable status explanation            |

## Commit Counting Logic

The `commits_behind` field counts commits chronologically between the system's current deployment and the latest commit in the same flake. This count includes **all** commits regardless of their evaluation or build status.

## Operational Implications

**Behind Systems**: May be running older code with missing features, bug fixes, or security updates. The `commits_behind` count indicates deployment lag severity.

**Unknown Systems**: Deployments that can't be traced to source control, making it impossible to assess update status or security posture.

**No Deployment**: Registered systems that haven't received their initial deployment, indicating incomplete provisioning.

## Use Cases

This view supports:

- **Update Planning**: Identify systems needing updates and prioritize by commits_behind count
- **Deployment Tracking**: Monitor deployment velocity and identify systems lagging behind
- **Security Assessment**: Find systems running older code that may contain known vulnerabilities
- **Infrastructure Auditing**: Ensure all systems can be traced to source control
- **Release Management**: Track rollout progress of new commits across the fleet

## Query Examples

```sql
-- Systems most in need of updates
SELECT hostname, commits_behind, current_commit_hash, latest_commit_hash
FROM view_system_deployment_status
WHERE deployment_status = 'behind'
ORDER BY commits_behind DESC;

-- Deployment status summary
SELECT deployment_status, COUNT(*) as system_count
FROM view_system_deployment_status
GROUP BY deployment_status;

-- Systems deployed in the last 24 hours
SELECT hostname, deployment_status, deployment_time
FROM view_system_deployment_status
WHERE deployment_time > NOW() - INTERVAL '24 hours'
ORDER BY deployment_time DESC;

-- Find systems that can't be traced to source control
SELECT hostname, current_derivation_path, status_description
FROM view_system_deployment_status
WHERE deployment_status IN ('unknown', 'no_deployment');
```

## Notes

- The view considers chronological commit order, not evaluation success - a system may be "behind" even if newer commits failed to build
- Systems with failed evaluations for newer commits will show as "behind" since the deployment view tracks availability, not deployability
- Results are ordered to show problematic deployments first (no_deployment, unknown, behind) for operational triage
