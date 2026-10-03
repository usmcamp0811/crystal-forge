# System Deployment Status View

## Overview

The `view_system_deployment_status` compares each system's latest observed store path with the newest deployable NixOS derivation for its registered flake and effective configuration name. It does not use raw repository HEAD as a deployment target. System list and detail views read this shared status.

## Status Categories

Systems are classified against the newest deployable build for their own configuration:

- **`up_to_date`**: The observed store path equals the newest deployable build's store path
- **`behind`**: The observed build maps to this flake and configuration, and a newer deployable build is available
- **`no_deployment`**: System is registered but has never been deployed
- **`unknown`**: The observed path cannot be compared to an eligible target (including when no deployable target exists)
- **`ahead`**: The observed build maps to a newer commit than the newest deployable build

## Key Relationships

The view traces the deployment chain:

1. The newest `system_states` row by `timestamp DESC NULLS LAST, id DESC` identifies the observed `store_path`.
2. The registered system's flake and `COALESCE(NULLIF(BTRIM(system_configuration_name), ''), hostname)` select eligible NixOS derivations.
3. An eligible derivation has a nonblank actual `store_path`, `cf_agent_enabled IS TRUE`, `policy_requirements_met IS TRUE`, no recorded derivation error, and a completed cache push whose `store_path` exactly matches that output. Source archival does not remove a built cached artifact. Evaluation-only paths and runtime deployment gates do not qualify a target.
4. Eligible candidates are ranked by commit timestamp descending, derivation completion descending (nulls last), then derivation ID descending. If multiple derivations record the observed path, the running-path mapping prefers an eligible exact cache-published NixOS derivation for the registered flake and configuration before a later failed record with the same path. Historical mapping remains available when no such eligible record exists. Unregistered state rows remain visible without multiplying rows.

## Important Fields

| Field                      | Description                                  |
| -------------------------- | -------------------------------------------- |
| `hostname`                 | System identifier                            |
| `deployment_status`        | Current deployment status                    |
| `commits_behind`           | Distinct newer eligible commit IDs between the observed and target commit timestamps |
| `current_commit_hash`      | Git hash of currently deployed commit        |
| `latest_commit_hash`       | Git hash of newest deployable target's commit |
| `current_commit_timestamp` | When current commit was made                 |
| `latest_commit_timestamp`  | When target commit was made                  |
| `deployment_time`          | When current deployment occurred             |
| `flake_name`               | Associated flake/repository name             |
| `status_description`       | Human-readable status explanation            |

## Commit Counting Logic

`commits_behind` counts distinct commit IDs with at least one eligible derivation for this configuration, newer than the observed commit and no newer than the target commit. Failed, pending, host-missing, policy-failed, and cache-incomplete commits do not count. A different deployable derivation at the same commit timestamp can produce `behind` with a zero count; the description does not assert a numeric distance.

## Operational Implications

**Behind Systems**: May be running older code with missing features, bug fixes, or security updates. The `commits_behind` count indicates deployment lag severity.

**Unknown Systems**: Deployments that can't be traced to source control, making it impossible to assess update status or security posture.

**No Deployment**: Registered systems that haven't received their initial deployment, indicating incomplete provisioning.

## Use Cases

This view supports:

- **Update Planning**: Identify systems with a deployable update and prioritize by eligible commits_behind count
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
SELECT hostname, current_store_path, status_description
FROM view_system_deployment_status
WHERE deployment_status IN ('unknown', 'no_deployment');
```

## Notes

- Failed evaluations and builds cannot make a system `behind`. A system without any deployable target cannot be `up_to_date`.
- The view does not guarantee row order. Callers that need a triage order must specify `ORDER BY`.
