# System Heartbeat Status View

## Overview

The `view_system_heartbeat_status` provides a focused health assessment of systems based on both agent heartbeats and system state changes. This view determines whether systems are actively responding, experiencing issues, or are unreachable.

## Status Categories

The view classifies systems into three heartbeat status categories:

- **`online`**: System has had either a heartbeat OR state change within the last 30 minutes
- **`stale`**: Most recent activity (heartbeat or state change) was between 30 minutes and 1 hour ago
- **`offline`**: No activity for over 1 hour, or system has never been seen

## Logic

The view uses a permissive approach where **either** recent heartbeat activity **or** recent system state changes indicate an active system. This recognizes that:

- A system actively changing state is clearly functional even if heartbeats are delayed
- Agent heartbeats may be disrupted while the system continues operating
- System state changes provide an independent signal of system activity

## Key Fields

| Field                         | Description                                         |
| ----------------------------- | --------------------------------------------------- |
| `hostname`                    | System identifier                                   |
| `heartbeat_status`            | Current status: `online`, `stale`, or `offline`     |
| `most_recent_activity`        | Latest timestamp between heartbeat and state change |
| `last_heartbeat`              | Most recent agent heartbeat (may be NULL)           |
| `last_state_change`           | Most recent system state update                     |
| `minutes_since_last_activity` | Time elapsed since most recent activity             |
| `status_description`          | Human-readable status explanation                   |

## Time Boundaries

- **15 minutes**: boundary between `Healthy` and `Warning`
- **1 hour**: boundary between `Warning` and `Critical`
- **4 hours**: boundary between `Critical` and `Offline`

**Status ranges**

- `Healthy`: last activity ≤ 15 minutes ago
- `Warning`: 15–60 minutes ago
- `Critical`: 1–4 hours ago
- `Offline`: > 4 hours ago

## Use Cases

This view is designed for:

- **Fleet health monitoring**: Quick assessment of which systems need attention
- **Alert prioritization**: Focus on `offline` systems first, then `stale`
- **Operational dashboards**: Real-time system availability status
- **Troubleshooting**: Understanding when systems last showed activity

## Related Views

While `view_systems_status_table` provides comprehensive system information including deployment status, this view focuses specifically on connectivity and responsiveness, making it ideal for health monitoring scenarios where you need a clear signal about system availability.

## Query Examples

```sql
-- Systems requiring immediate attention
SELECT hostname, heartbeat_status, minutes_since_last_activity
FROM view_system_heartbeat_status
WHERE heartbeat_status IN ('offline', 'stale')
ORDER BY minutes_since_last_activity DESC;

-- Healthy systems count
SELECT heartbeat_status, COUNT(*)
FROM view_system_heartbeat_status
GROUP BY heartbeat_status;

-- Systems with heartbeat issues but recent state changes
SELECT hostname, last_heartbeat, last_state_change, heartbeat_status
FROM view_system_heartbeat_status
WHERE last_heartbeat < (NOW() - INTERVAL '30 minutes')
  AND last_state_change > (NOW() - INTERVAL '30 minutes')
  AND heartbeat_status = 'online';
```
