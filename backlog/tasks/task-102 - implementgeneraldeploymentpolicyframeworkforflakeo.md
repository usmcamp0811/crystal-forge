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
-->\n\n# Issue Details\n\n- **Issue ID:** 174147608\n- **Issue IID:** 102\n- **Title:** Implement General Deployment Policy Framework for Flake Outputs\n- **State:** closed\n- **Labels:** epic::security-policy\n- **Created by:** Matt\n- **Created at:** 2025-09-30T12:26:06.669Z\n- **Updated at:** 2025-11-06T01:45:09.902Z\n\n# Description\n\n## Background

Currently, Crystal Forge has a basic gate that prevents deployment of NixOS derivations without the Crystal Forge agent enabled (`cf_agent_enabled` check in `is_deployment_safe()`). This works but is limited to a single hardcoded condition.

We need a more general, extensible policy framework that allows defining arbitrary conditions that must be met before a flake output is considered deployable.

## Motivation

- **Compliance Requirements**: Organizations may require specific security modules (e.g., STIG configurations) to be enabled
- **Safety Gates**: Beyond CF agent, there may be other critical services or configurations that must be present
- **Auditability**: Clear, declarative policies that can be reviewed and version-controlled
- **Flexibility**: Different systems may have different policy requirements

## Proposed Solution

### Phase 1: Policy Definition Framework

Create a policy definition system that can evaluate arbitrary conditions on NixOS configurations:

```rust
// models/policies.rs
pub struct DeploymentPolicy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub policy_type: PolicyType,
    pub required: bool,
    pub evaluation_expr: String, // Nix expression to evaluate
}

pub enum PolicyType {
    ModuleEnabled,      // Check if a NixOS module is enabled
    ServiceRunning,     // Check if a service is configured to run
    ConfigValue,        // Check a specific config value
    Custom,             // Custom Nix expression
}
```

### Phase 2: Policy Evaluation

Extend the derivation evaluation process to check policies:

```rust
// In build.rs or new policies.rs
pub async fn evaluate_deployment_policies(
    flake_target: &str,
    build_config: &BuildConfig,
) -> Result<PolicyEvaluationResult> {
    // Load applicable policies from database
    let policies = load_policies_for_target(flake_target).await?;
    
    let mut results = Vec::new();
    for policy in policies {
        let result = evaluate_policy(flake_target, &policy, build_config).await?;
        results.push(result);
    }
    
    Ok(PolicyEvaluationResult {
        all_passed: results.iter().all(|r| r.passed),
        policy_results: results,
    })
}
```

### Phase 3: Database Schema

Add tables to store and manage policies:

```sql
CREATE TABLE deployment_policies (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    policy_type TEXT NOT NULL,
    evaluation_expr TEXT NOT NULL,
    required BOOLEAN NOT NULL DEFAULT true,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE system_policies (
    id SERIAL PRIMARY KEY,
    system_id INTEGER REFERENCES systems(id) ON DELETE CASCADE,
    policy_id INTEGER REFERENCES deployment_policies(id) ON DELETE CASCADE,
    override_required BOOLEAN, -- Override policy's default required setting
    UNIQUE(system_id, policy_id)
);

CREATE TABLE derivation_policy_results (
    id SERIAL PRIMARY KEY,
    derivation_id INTEGER REFERENCES derivations(id) ON DELETE CASCADE,
    policy_id INTEGER REFERENCES deployment_policies(id) ON DELETE CASCADE,
    passed BOOLEAN NOT NULL,
    evaluation_output TEXT,
    evaluated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### Phase 4: Example Policies

**Crystal Forge Agent (migrate existing check):**
```nix
let 
  cfg = config.services.crystal-forge or {};
in
  cfg.enable && cfg.client.enable
```

**STIG Hardening:**
```nix
config.security.stig.enable or false
```

**Firewall Enabled:**
```nix
config.networking.firewall.enable
```

**SSH Key-Only Authentication:**
```nix
!config.services.openssh.settings.PasswordAuthentication
```

**Audit Logging:**
```nix
config.services.auditd.enable or false
```

## Implementation Plan

### Step 1: Core Framework (Week 1-2)
- [ ] Create `models/policies.rs` with policy data structures
- [ ] Add database migrations for policy tables
- [ ] Implement basic policy loading from database
- [ ] Create queries for policy management

### Step 2: Policy Evaluation (Week 2-3)
- [ ] Implement `evaluate_policy()` function using Nix eval
- [ ] Handle different policy types (ModuleEnabled, ServiceRunning, etc.)
- [ ] Store evaluation results in database
- [ ] Update `is_deployment_safe()` to check policy results

### Step 3: Migrate Existing Check (Week 3)
- [ ] Create default CF agent policy in database
- [ ] Migrate `cf_agent_enabled` check to use new framework
- [ ] Ensure backward compatibility during transition
- [ ] Update tests

### Step 4: UI/API (Week 4)
- [ ] Add API endpoints for policy CRUD operations
- [ ] Display policy evaluation results in derivation details
- [ ] Add system policy configuration UI
- [ ] Show policy compliance status on system overview

### Step 5: Documentation & Testing (Week 5)
- [ ] Document policy creation and management
- [ ] Add examples for common security policies
- [ ] Integration tests for policy evaluation
- [ ] Update deployment docs

## Future Enhancements

- **Policy Templates**: Pre-built policies for common compliance frameworks (CIS, STIG, etc.)
- **Policy Groups**: Bundle related policies (e.g., "Production Security Baseline")
- **Warnings vs. Blockers**: Differentiate between hard failures and warnings
- **Policy Inheritance**: System groups inherit policies from parent groups
- **Time-based Policies**: Grace periods for new policy rollouts
- **Policy Exceptions**: Temporary waivers with approval workflow

## Success Criteria

- [ ] Can define arbitrary deployment policies without code changes
- [ ] Existing CF agent check migrated to new framework
- [ ] Policy evaluation results visible in UI
- [ ] At least 3 example security policies implemented
- [ ] Documentation complete with examples
- [ ] Zero regression in existing deployment behavior

## Related Issues

- #XXX: CF Agent Deployment Gate (current implementation)
- #XXX: Security Compliance Dashboard
- #XXX: STIG Module Integration

## References

- Current implementation: `models/derivations/mod.rs:is_deployment_safe()`
- Policy evaluation location: `models/derivations/build.rs:is_cf_agent_enabled()`\n\n# Milestone\n\n{
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