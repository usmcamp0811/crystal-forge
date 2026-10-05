---
type: Operator Guide
title: "Observability and troubleshooting"
description: "Lists observability points, common derivation pipeline issues with fixes, key metrics, and alerting thresholds; open it when monitoring the fleet or diagnosing stuck or failing derivations."
tags:
  - crystal-forge
  - operations
  - observability
  - troubleshooting
  - monitoring
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:15-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file docs/architecture.md at commit 3b23d36f"
    title: "ADR-000: Crystal Forge Architecture Overview"
  - id: s2
    resource: "Crystal Forge repository file docs/derivation-status.md at commit 3b23d36f"
    title: "Crystal Forge Derivation Status Flow"
---

# Observability and Troubleshooting

> **Status:** partial. Status IDs and names in "Common Issues" follow [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md). Alert thresholds are recommendations, not enforced behavior. Whether the recommended metrics are exported has not been checked.

## Observability Points

1. **Agent health monitoring**: Heartbeat frequency, signature validation success rate
2. **Build coordination metrics**: Evaluation times, CVE scan duration, queue depth
3. **Compliance metrics**: Systems in drift, CVE exposure levels, STIG compliance rates
4. **Database performance**: Query times, connection counts, replication lag

## Common Issues

### **Stuck in pending (1)**
- **Fix:** Set to `dry-run-pending` (3)
- **Cause:** Usually startup state or manual insertion

### **Not processing**
- **Status 3:** Check evaluation loop health
- **Status 5 or 7:** Check build loop health
- **Check:** Loop intervals and resource availability

### **High failure rates**
- **Check:** `attempt_count` (≥ 5 = permanent failure)
- **Inspect:** `error_message` for root cause
- **Common causes:** Resource limits, network issues, configuration errors

### **Cache Push Failures**
- **Check:** Cache credentials and connectivity
- **Verify:** Store path exists and is readable
- **Review:** Cache configuration and filters

### **Deployment Issues**
- **Verify:** Agent connectivity and authentication
- **Check:** Deployment policy configuration
- **Review:** nixos-rebuild logs on target systems

## Monitoring Recommendations

### **Key Metrics**
- Derivation processing rate by status
- Build success/failure ratios
- Cache push success rates
- Deployment success rates
- CVE scan coverage

### **Alerting Thresholds**
- Derivations stuck in pending > 1 hour
- Build failure rate > 20%
- Cache push failure rate > 10%
- Agent heartbeat gaps > 30 minutes
- High-severity CVEs in deployed systems

## Related concepts

- [Derivation status lifecycle](../concepts/derivation-status-lifecycle.md) - status IDs referenced above
- [Derivation processing loops](../architecture/derivation-processing-loops.md) - loop intervals to check when nothing processes
- [Cache push process](../caches/cache-push-process.md) - cache push behavior behind push failures
