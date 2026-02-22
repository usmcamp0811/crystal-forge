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
-->\n\n# Issue Details\n\n- **Issue ID:** 175530705\n- **Issue IID:** 105\n- **Title:** Detach nixos-rebuild Execution to Prevent Self-Termination\n- **State:** closed\n- **Labels:** feature::config-deployment\n- **Created by:** Matt\n- **Created at:** 2025-10-19T19:36:10.673Z\n- **Updated at:** 2025-11-06T01:48:35.632Z\n\n# Description\n\n### Problem
The Crystal Forge agent currently runs `nixos-rebuild switch` directly, which causes the agent service to be killed during the rebuild when the service definition is updated. This prevents the agent from automatically restarting after a successful deployment.

### Solution
Detach the `nixos-rebuild switch` execution using `systemd-run` so that the rebuild continues even after the parent agent process is terminated.

### Implementation Steps

#### 1. Modify `AgentDeploymentManager::execute_deployment()` 
**File:** `crates/agent/src/deployment/agent.rs` (or wherever the deployment execution logic lives)

**Current behavior:** The agent directly calls `nixos-rebuild switch`

**New behavior:** Use `systemd-run` to detach the rebuild process:

```rust
// Instead of:
Command::new("nixos-rebuild")
    .args(["switch", "--flake", flake_uri])
    .status()?

// Change to:
let timestamp = chrono::Utc::now().timestamp();
let unit_name = format!("crystal-forge-deploy-{}", timestamp);

Command::new("systemd-run")
    .arg("--unit")
    .arg(&unit_name)
    .arg("--no-block")
    .arg("--same-dir")
    .arg("--collect")  // Auto-cleanup the transient unit
    .arg("--")
    .arg("nixos-rebuild")
    .arg("switch")
    .arg("--flake")
    .arg(flake_uri)
    .status()?
```

#### 2. Add Deployment Tracking Logic

Since the deployment is now detached, we need to track it differently:

```rust
// After spawning the systemd-run command
info!("Deployment detached to systemd unit: {}", unit_name);

// Return immediately with a "deployment started" status
DeploymentResult::Started {
    unit_name,
    timestamp,
}
```

#### 3. Update `DeploymentResult` Enum

```rust
pub enum DeploymentResult {
    Success { derivation_path: String },
    Failed { error: String },
    Started { unit_name: String, timestamp: i64 },  // NEW
    Skipped { reason: String },
}
```

#### 4. Add systemd to Agent's PATH

**File:** Your NixOS module (the one you showed earlier)

Ensure `systemd` is in the agent's path (it already is based on your module, but verify):

```nix
path = with pkgs; [
    # ... existing packages ...
    systemd  # Already present - just verify
];
```

#### 5. Handle Post-Deployment State

The agent needs to check if a detached deployment succeeded on the next heartbeat:

```rust
// On startup or heartbeat, check if there's a recent deployment unit
async fn check_recent_deployment(&self) -> Result<Option<DeploymentResult>> {
    // Query systemd for crystal-forge-deploy-* units from the last 30 minutes
    // Check their status (succeeded/failed)
    // Return result if found
}
```



### Expected Behavior

- Agent receives deployment command from server
- Agent spawns `systemd-run` with `nixos-rebuild switch`
- Agent returns immediately with "deployment started" status
- System performs the rebuild (updating the agent service)
- Systemd stops the old agent service and starts the new one
- Agent comes back online and reports the new system state on next heartbeat

### Acceptance Criteria

- [ ] Agent successfully detaches `nixos-rebuild switch` using `systemd-run`
- [ ] Agent service automatically restarts after a self-update deployment
- [ ] Deployment continues to completion even after agent is killed
- [ ] Agent reports deployment status on next heartbeat
- [ ] No orphaned processes or systemd units left behind
- [ ] Error handling for failed detached deployments

### Notes

- The `--collect` flag tells systemd to automatically clean up the transient unit after it finishes
- Consider adding logging to track deployment units: `journalctl -u crystal-forge-deploy-*`
- May want to add a cleanup routine to remove old deployment units if `--collect` doesn't work as expected

**Risk:** Low - systemd-run is well-tested and reliable  
**Dependencies:** None - uses existing systemd infrastructure\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n