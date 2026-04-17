# Title

<!--
Short, outcome-focused title
-->

---

# Problem

<!--
Brief description of the issue or opportunity.
Keep this lightweight.
-->

---

# Desired Outcome

<!--
What should be true if this is completed?
-->

---

# Notes

<!--
Optional context, links, screenshots, or references.
-->

---

# Scope Hint (Optional)

<!--
If obvious, describe rough boundaries.
Not required at Backlog stage.
-->\n\n# Issue Details\n\n- **Issue ID:** 171324918\n- **Issue IID:** 85\n- **Title:** Add DoD STIG-Compliant Security Logging to Crystal Forge\n- **State:** opened\n- **Labels:** compliance::nist, compliance::rmf, compliance::stig\n- **Created by:** Matt\n- **Created at:** 2025-07-30T20:08:52.597Z\n- **Updated at:** 2025-07-30T20:10:11.979Z\n\n# Description\n\n## Objective
Extend existing Crystal Forge tracing infrastructure with structured security audit logging that meets DoD STIG requirements for authentication, authorization, and system events.

## Background
Crystal Forge already uses `tracing` for logging. This task adds a security audit layer on top of existing infrastructure to capture STIG-required events without disrupting current functionality.

## Scope

### Core Implementation
- [ ] **Security Audit Module** (`src/audit/mod.rs`)
  - Structured audit event types (Authentication, Authorization, System, Network)
  - STIG-compliant event formatter (JSON + syslog RFC 5424)
  - Integration with existing `tracing` infrastructure

- [ ] **Agent Security Events** (extend `agent.rs`)
  - Authentication events: key validation, signature verification
  - Authorization events: system state updates, heartbeat authorization
  - System events: configuration changes, service start/stop

- [ ] **Server Security Events** (extend `server.rs` handlers)
  - Authentication events: agent key verification failures/successes
  - Authorization events: endpoint access, webhook validation
  - Network events: connection attempts, rate limiting

### Required Audit Events (STIG Subset)
```rust
pub enum SecurityEventType {
    Authentication { success: bool, key_id: String },
    Authorization { resource: String, action: String, outcome: String },
    SystemChange { component: String, change_type: String },
    NetworkActivity { source_ip: String, endpoint: String, status: u16 },
}
```

### Integration Points
- **Agent**: Audit signature verification, system state changes
- **Server**: Audit webhook processing, agent authentication
- **Builder**: Audit build start/completion, CVE scan results
- **Key Generation**: Audit key creation events

### Technical Approach
```rust
// Extend existing tracing with security layer
use tracing::{info, instrument};

#[instrument]
pub fn audit_authentication(key_id: &str, success: bool, context: &str) {
    let event = SecurityEvent::authentication(key_id, success, context);
    // Log to both tracing and structured audit log
    info!(
        event.type = "authentication",
        event.outcome = if success { "success" } else { "failure" },
        key.id = key_id,
        "Agent authentication attempt"
    );
    AUDIT_LOGGER.log_security_event(event);
}
```

### Deliverables
- [ ] Security audit module with STIG event types
- [ ] Integration in 3-4 key functions (agent auth, state updates, webhook processing)
- [ ] NixOS configuration for audit log collection
- [ ] Basic compliance validation tests
- [ ] Documentation for security administrators

### Expected Log Output Examples

**Agent Authentication Success:**
```json
{
  "timestamp": "2025-07-30T15:45:23.123456Z",
  "event_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "event_type": "authentication",
  "severity": "info",
  "outcome": "success",
  "subject": {
    "type": "agent",
    "id": "nixos-build-01",
    "ip_address": "192.168.1.100"
  },
  "object": {
    "type": "server_endpoint",
    "resource": "/agent/state"
  },
  "message": "Agent authentication successful",
  "additional_data": {
    "signature_valid": true,
    "key_id": "nixos-build-01"
  },
  "source": {
    "hostname": "crystal-forge-server",
    "component": "authentication_handler"
  }
}
```

**System Configuration Change:**
```json
{
  "timestamp": "2025-07-30T15:50:12.456789Z",
  "event_type": "system_change",
  "severity": "info",
  "outcome": "success",
  "subject": {
    "type": "system",
    "id": "nixos-build-01"
  },
  "object": {
    "type": "system_configuration",
    "resource": "/nix/store/abc123-nixos-system-24.05",
    "previous_value": "/nix/store/xyz789-nixos-system-24.05"
  },
  "message": "System configuration changed",
  "additional_data": {
    "change_reason": "config_change",
    "derivation_hash": "abc123def456",
    "trigger": "inotify_event"
  }
}
```

**Build Process Event:**
```json
{
  "timestamp": "2025-07-30T16:00:15.654321Z",
  "event_type": "system_change",
  "severity": "info",
  "outcome": "success",
  "subject": {
    "type": "build_process",
    "id": "builder-daemon"
  },
  "object": {
    "type": "build_target",
    "resource": "nixos-laptop",
    "action": "dry_run_complete"
  },
  "message": "Build evaluation completed",
  "additional_data": {
    "commit_hash": "a1b2c3d4e5f6789",
    "target_type": "nixos",
    "derivation_path": "/nix/store/def456-nixos-laptop-system",
    "build_duration_ms": 45000
  }
}
```

**CVE Scan Alert:**
```json
{
  "timestamp": "2025-07-30T16:05:22.111222Z",
  "event_type": "system_change",
  "severity": "high",
  "outcome": "success",
  "subject": {
    "type": "security_scanner",
    "id": "vulnix-scanner"
  },
  "object": {
    "type": "system_derivation",
    "resource": "/nix/store/def456-nixos-laptop-system",
    "action": "vulnerability_scan"
  },
  "message": "CVE scan completed with vulnerabilities found",
  "additional_data": {
    "vulnerabilities_found": 3,
    "critical_count": 1,
    "high_count": 2,
    "scanner_version": "vulnix-1.10.1"
  }
}
```

### Acceptance Criteria
- Authentication events logged for all agent interactions
- System change events logged for configuration updates
- Audit logs include all STIG-required data elements (timestamp, user, outcome, etc.)
- No performance impact >2% on existing functionality
- Audit logs in structured format (JSON) for SIEM integration
- Log format supports multiple outputs (JSON, syslog RFC 5424, CEF)

### Dependencies
- Existing Crystal Forge codebase (agent.rs, server.rs, builder.rs)
- Current tracing infrastructure
- NixOS audit configuration

### Out of Scope (Future Sprints)
- Log integrity/tamper protection
- Advanced SIEM integration
- Complete STIG compliance validation
- Log retention policies

---

**Estimated Effort: 5-8 days**
**Priority: Medium** - Foundation for enterprise security requirements\n\n# Milestone\n\n{
  "id": 6040389,
  "iid": 5,
  "group_id": 0,
  "project_id": 70402481,
  "title": "v0.4.0 - Enterprise Security",
  "description": "**Goal**: Prepare for LAN or production deployment.\r\n\r\n* [x] Shared secret or token auth for agents\r\n* [ ] Harden webhook input (validate source repo)\r\n* [ ] Limit derivation evaluation to allowlist\r\n* [ ] Add systemd service definition + journald config\r\n* [ ] Write tests for all HTTP handlers + flake logic",
  "start_date": null,
  "due_date": null,
  "state": "active",
  "web_url": "https://gitlab.com/crystal-forge/crystal-forge/-/milestones/5",
  "updated_at": "2025-06-28T02:47:15.836Z",
  "created_at": "2025-06-14T03:46:26.876Z",
  "expired": false
}\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n