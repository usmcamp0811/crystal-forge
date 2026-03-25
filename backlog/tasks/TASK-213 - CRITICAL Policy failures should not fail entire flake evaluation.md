---
id: TASK-213
title: CRITICAL - Policy failures should not fail entire flake evaluation
status: Review
created: 2026-03-22
priority: critical
tags: [bug, critical, policies, evaluation, blocking]
risk: medium
notes: |
  LOCK: claude-agent on gray in ~/code/crystal-forge/TASK-213-policy-failures-per-system
  Started: 2026-03-24
  Completed: 2026-03-24
  MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/184
  
  Implementation:
  - Removed bail!() that was blocking entire evaluation on policy failures
  - Added per-system logging (ERROR for strict, WARN for non-strict, INFO for passed)
  - Added summary statistics showing X/Y systems passed
  - Build verified: nix build .#default passes
  
  Awaiting merge to dev for production verification.
---

## Problem

Currently, if **any single system** in a flake fails a strict policy check, the **entire flake evaluation fails** and **all systems** (including those that passed policies) are not processed.

**Current broken behavior:**
- Flake has 10 systems: 9 pass all policies, 1 fails a policy
- Entire evaluation fails with error
- All 10 systems marked as failed
- None of the 9 passing systems get queued for build
- Commit marked as "failed" instead of "complete with warnings"

**Expected correct behavior:**
- Each system should be evaluated independently
- Systems that pass policies should be queued for build
- Systems that fail policies should be marked as policy-failed but not block others
- Evaluation should complete successfully with mixed results
- Commit marked as "complete" with summary of pass/fail counts

**Current code location:**
`packages/default/src/models/evaluate_with_policies.rs` lines 414-432

```rust
// Check for strict policy failures
let strict_failures: Vec<_> = policy_checks
    .iter()
    .filter(|c| !c.meets_requirements && policies.iter().any(|p| p.is_strict()))
    .collect();

if !strict_failures.is_empty() {
    bail!("{} systems failed strict deployment policies", strict_failures.len());
}
```

This `bail!()` causes the entire evaluation to fail, preventing successful systems from being processed.

## Goal

Systems should be evaluated independently. Policy failures for one system should not affect other systems in the same flake.

## Desired Outcome

**After fix:**
1. Evaluation runs for all systems in flake
2. Systems that pass policies → marked as `cf_agent_enabled=true`, queued for build
3. Systems that fail non-strict policies → marked as `cf_agent_enabled=false`, logged as warning
4. Systems that fail strict policies → marked as `cf_agent_enabled=false`, logged as error
5. Evaluation completes successfully regardless of individual system policy results
6. Commit marked as "complete" with summary stats

**Database state after evaluation:**
```sql
-- Example: 10 systems, 8 pass, 2 fail policy
derivations table:
  - 8 rows with cf_agent_enabled=true, status=queued_for_build
  - 2 rows with cf_agent_enabled=false, status=policy_failed or similar

commits table:
  - evaluation_status='complete'
  - Summary shows: "8 systems passed, 2 failed policy"
```

## Non-Goals

- Changing policy strictness semantics (strict policies should still prevent builds)
- Changing policy evaluation logic (policies are correctly evaluated)
- Auto-fixing policy failures (systems should fail if they don't meet requirements)

## Acceptance Criteria

**Critical (must have):**
- [ ] Remove the `bail!()` that fails entire evaluation on strict policy failures
- [ ] All systems in flake are processed regardless of individual policy results
- [ ] Systems that pass policies are queued for build
- [ ] Systems that fail policies are marked appropriately (cf_agent_enabled=false)
- [ ] Evaluation completes with status='complete' even if some systems fail policy
- [ ] Commit summary shows count of passed vs failed systems

**Important (should have):**
- [ ] Log ERROR for systems that fail strict policies
- [ ] Log WARNING for systems that fail non-strict policies  
- [ ] UI shows per-system policy results clearly
- [ ] WebSocket broadcasts show per-system status correctly

**Nice to have:**
- [ ] Commit detail view shows breakdown: "8/10 systems passed policies"
- [ ] Filter/search for systems that failed policies
- [ ] Policy failure reasons included in system metadata

## Implementation Plan

### Phase 1: Remove Blocking Behavior (CRITICAL)

**File:** `packages/default/src/models/evaluate_with_policies.rs`

**Current code (lines 414-432):**
```rust
// Check for strict policy failures
let strict_failures: Vec<_> = policy_checks
    .iter()
    .filter(|c| !c.meets_requirements && policies.iter().any(|p| p.is_strict()))
    .collect();

if !strict_failures.is_empty() {
    error!("{}", strict_failures.len());
    for failure in &strict_failures {
        error!("  - {}", failure.system_name);
        for warning in &failure.warnings {
            error!("    • {}", warning);
        }
    }
    bail!(
        "{} systems failed strict deployment policies",
        strict_failures.len()
    );
}
```

**New code:**
```rust
// Log strict policy failures but DON'T fail entire evaluation
let strict_failures: Vec<_> = policy_checks
    .iter()
    .filter(|c| !c.meets_requirements && policies.iter().any(|p| p.is_strict()))
    .collect();

if !strict_failures.is_empty() {
    error!(
        "⚠️  {} systems failed strict deployment policies (will not be queued for build):",
        strict_failures.len()
    );
    for failure in &strict_failures {
        error!("  - {}", failure.system_name);
        for warning in &failure.warnings {
            error!("    • {}", warning);
        }
    }
    // DO NOT bail!() - let evaluation continue
}

// Log non-strict policy failures as warnings
let non_strict_failures: Vec<_> = policy_checks
    .iter()
    .filter(|c| !c.meets_requirements && !policies.iter().any(|p| p.is_strict()))
    .collect();

if !non_strict_failures.is_empty() {
    warn!(
        "⚠️  {} systems failed non-strict deployment policies:",
        non_strict_failures.len()
    );
    for failure in &non_strict_failures {
        warn!("  - {}", failure.system_name);
        for warning in &failure.warnings {
            warn!("    • {}", warning);
        }
    }
}

// Log systems that passed policies
let passed_systems: Vec<_> = policy_checks
    .iter()
    .filter(|c| c.meets_requirements)
    .collect();

if !passed_systems.is_empty() {
    info!(
        "✅ {} systems passed all deployment policies",
        passed_systems.len()
    );
}
```

### Phase 2: Update Evaluation Summary

**File:** `packages/default/src/server/mod.rs` (around line 450-480)

After evaluation completes, add summary statistics:

```rust
let passed_count = policy_checks.iter().filter(|c| c.meets_requirements).count();
let failed_count = policy_checks.len() - passed_count;

if failed_count > 0 {
    warn!(
        "📊 Evaluation complete: {}/{} systems passed policies, {} failed",
        passed_count,
        policy_checks.len(),
        failed_count
    );
} else {
    info!(
        "📊 Evaluation complete: all {} systems passed policies",
        policy_checks.len()
    );
}
```

Broadcast this to WebSocket clients for UI display.

### Phase 3: Update UI Display (if needed)

Ensure the UI correctly shows:
- Per-system policy status
- Systems with cf_agent_enabled=false shown as "Policy Failed" or similar
- Systems with cf_agent_enabled=true shown as "Queued for Build"
- Overall commit status as "Complete" not "Failed"

## Verification Plan

**Test Case 1: Mixed policy results**
1. Create a flake with 3 systems:
   - system1: has CF agent enabled (should pass)
   - system2: no CF agent (should fail require_cf_agent)
   - system3: has CF agent enabled (should pass)
2. Evaluate the commit
3. Verify:
   - Evaluation completes with status='complete'
   - system1 has cf_agent_enabled=true, queued for build
   - system2 has cf_agent_enabled=false, not queued
   - system3 has cf_agent_enabled=true, queued for build
   - Logs show "2/3 systems passed policies"

**Test Case 2: All systems fail policy**
1. Create a flake with 3 systems, none have CF agent
2. Evaluate the commit
3. Verify:
   - Evaluation completes with status='complete' (not 'failed')
   - All 3 systems have cf_agent_enabled=false
   - None queued for build
   - Logs show "0/3 systems passed policies"

**Test Case 3: All systems pass policy**
1. Create a flake with 3 systems, all have CF agent
2. Evaluate the commit  
3. Verify:
   - Evaluation completes successfully
   - All 3 systems queued for build
   - Logs show "3/3 systems passed policies"

**Verification Commands:**

After evaluation:
```sql
-- Check system-level results
SELECT 
    derivation_name,
    cf_agent_enabled,
    status_id
FROM derivations
WHERE commit_id = <commit_id>
ORDER BY derivation_name;

-- Check overall commit status
SELECT 
    evaluation_status,
    git_commit_hash
FROM commits
WHERE id = <commit_id>;
```

## Files to Modify

**Primary:**
- `packages/default/src/models/evaluate_with_policies.rs` - Remove bail!(), add logging

**Secondary (if needed):**
- `packages/default/src/server/mod.rs` - Add evaluation summary
- `packages/web-ui/src/pages/commits.rs` - Update UI to show per-system status (if not already)

## Impact Analysis

**Positive:**
- HIGH: Unblocks systems that meet policies from being deployed
- HIGH: Reduces evaluation failures significantly
- MEDIUM: Better UX - users see which specific systems failed, not just "evaluation failed"
- LOW: More granular control over deployment policies

**Risk:**
- LOW: No breaking changes to database schema
- LOW: Existing policy logic unchanged
- MEDIUM: Need to ensure UI correctly displays mixed results

## Rollback Plan

If the fix causes issues, the original behavior can be restored by adding back the `bail!()`:

```rust
if !strict_failures.is_empty() {
    bail!("{} systems failed strict deployment policies", strict_failures.len());
}
```

This is a one-line revert.

## Related Issues

- TASK-212: Policy expression bug (currently blocking all evaluations)
- Future: Per-environment policy configuration
- Future: Policy override mechanism for emergency deployments

## Dependencies

- TASK-212 Phase 1 must be completed first (database fix for policy expressions)
- Otherwise evaluations fail with Nix errors before reaching policy check code

## Effort Estimate

**Small** (1-2 hours)
- Remove one `bail!()` call
- Add logging statements
- Add summary statistics
- Test with mixed results

## Architectural Constraints

- Must maintain per-system policy evaluation
- Must not change policy strictness semantics
- Must preserve cf_agent_enabled flag in derivations table
- Evaluation completion status must reflect success/failure correctly

## Testing Requirements

**Unit tests:**
- Test policy failure logging doesn't bail
- Test summary statistics are correct
- Test mixed pass/fail scenarios

**Integration tests:**
- Evaluate flake with mixed results
- Verify database state matches expectations
- Verify builds are queued for passing systems only

**Manual testing:**
- Use real flake with multiple systems
- Verify UI shows correct status per system
- Verify logs are clear and actionable

## Documentation Updates

After fix:
- Update deployment policies documentation to clarify per-system evaluation
- Document the difference between evaluation failure (Nix error) vs policy failure
- Add examples of mixed evaluation results in user guide
- Update troubleshooting section for policy failures

## Documentation Improvements (For AI Agents and Users)

The current policy system documentation is ambiguous about evaluation vs policy failure semantics. Add these clarifications:

### 1. Add to `docs/policies.md` (or create if missing):

**Section: "Policy Evaluation Model"**

```markdown
## Policy Evaluation Model

Crystal Forge evaluates deployment policies at the **per-system level**, not per-flake level.

### Key Principles

1. **Independent System Evaluation**: Each NixOS configuration in a flake is evaluated independently
2. **Policy Failures Are Per-System**: A system failing a policy does NOT fail the entire evaluation
3. **Evaluation Success vs Policy Success**: These are different concepts:
   - **Evaluation failure**: Nix expression error, syntax error, missing dependencies → Entire commit marked as "failed"
   - **Policy failure**: System doesn't meet deployment requirements → That system marked as "policy failed", others continue

### Example Scenario

Given a flake with 10 systems:
- 8 systems have Crystal Forge agent enabled
- 2 systems do NOT have Crystal Forge agent enabled

**Result:**
- ✅ Evaluation completes successfully (status: 'complete')
- ✅ 8 systems pass policies → queued for build → eligible for deployment
- ⚠️ 2 systems fail policies → NOT queued for build → NOT eligible for deployment
- 📊 Commit summary: "8/10 systems passed policies"

**NOT:**
- ❌ Evaluation fails
- ❌ All 10 systems marked as failed
- ❌ Commit marked as failed

### Policy Strictness

The `strict` flag on policies controls **logging severity**, not evaluation blocking:

- `strict: true` → Log as ERROR, system not queued for build
- `strict: false` → Log as WARNING, system not queued for build

Both prevent the system from building, but neither fails the overall evaluation.

### When Does Evaluation Fail?

Evaluation only fails for:
- Nix syntax errors
- Missing flake inputs
- Invalid attribute paths
- nix-eval-jobs crashes
- Database errors

Evaluation does NOT fail for:
- Individual systems failing policies
- Systems missing from flake
- Systems with evaluation errors (they're skipped)
```

### 2. Add to `AGENTS.md` (AI Agent Instructions):

**New Section: "Policy Evaluation Semantics"**

```markdown
## Policy Evaluation Semantics (CRITICAL FOR AI AGENTS)

### Evaluation Failure vs Policy Failure

**EVALUATION FAILURE** (code fails):
- Nix expression syntax error
- Missing dependencies
- Invalid flake reference
- nix-eval-jobs crash
→ Result: Entire commit marked as 'failed', no systems processed

**POLICY FAILURE** (system doesn't meet requirements):
- System missing Crystal Forge agent
- System missing required packages
- System fails custom policy check
→ Result: That system marked as policy-failed, OTHER SYSTEMS CONTINUE

### Implementation Requirement

When implementing policy-related features:
- Policy failures MUST NOT call `bail!()` or return `Err()`
- Policy failures MUST be logged and recorded per-system
- Evaluation MUST complete and return `Ok()` even with policy failures
- Systems are independently evaluated - one failure doesn't block others

### Correct Pattern

```rust
// ✅ CORRECT: Log policy failures, don't bail
if !system.meets_requirements {
    error!("System {} failed policy: {}", system.name, reason);
    mark_system_as_policy_failed(system);
    // Continue to next system
}

// ❌ WRONG: Bail on policy failure
if !system.meets_requirements {
    bail!("System failed policy"); // This blocks all other systems!
}
```

### Agent Reasoning Hints

When you see evaluation failure logs:
1. First check: Is this a Nix error or policy failure?
2. If policy failure: Verify only that system is affected, not entire flake
3. If evaluation failure: All systems in commit are affected

When designing policy features:
1. Always assume flakes have mixed results (some pass, some fail)
2. Never assume "all systems must pass" unless explicitly required
3. Treat systems as independent units of deployment
```

### 3. Update `CONTRIBUTING.md`:

**Add to "Design Principles" section:**

```markdown
### System-Level Independence

Each NixOS configuration in a flake is an independent deployment unit:
- Policy checks are per-system
- Build queuing is per-system
- Deployment is per-system
- Failures are per-system

Do NOT implement features that treat flakes as atomic units unless explicitly required for that feature (e.g., flake-level polling is fine, but evaluation should be per-system).
```
