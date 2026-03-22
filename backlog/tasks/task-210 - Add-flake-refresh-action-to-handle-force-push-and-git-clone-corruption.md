# Task 210: Add Flake Refresh Action to Handle Force-Push and Git Clone Corruption

**Status**: Backlog  
**Priority**: Medium  
**Risk**: Medium  
**Effort**: Medium (4-8 hours)  

---

## Problem

When a flake repository is force-pushed or has its git history rewritten, Nix's flake evaluation cache may retain stale references to commits that no longer exist. This can cause evaluation failures with errors like:

- "Invalid revision range"
- "Object not found" 
- "Reference does not exist"

Currently, there is **no way for users to refresh/re-clone a flake** to recover from this state. The only workaround is manual intervention on the server to clear Nix's flake cache or remove the cloned repository.

**Impact**: Flakes can become permanently stuck in an error state, blocking deployments.

---

## Goal

Provide a user-facing "Refresh Flake" action that:
1. Clears Nix's cached clone of the flake repository
2. Forces a fresh clone and re-evaluation
3. Recovers from force-push and git history corruption scenarios

---

## Non-Goals

- Automatic detection and recovery (should be manual/on-demand)
- Handling all types of evaluation errors (only git-related corruption)
- Preventing force-pushes (that's a git repository management concern)

---

## Acceptance Criteria

1. **API Endpoint**:
   - [ ] `POST /api/v1/flakes/{id}/refresh` endpoint exists
   - [ ] Requires Maintainer or Admin role
   - [ ] Endpoint triggers flake cache refresh logic
   - [ ] Returns success/failure status

2. **Backend Implementation**:
   - [ ] Clears Nix's flake evaluation cache for the specific flake URI
   - [ ] May use `nix flake update --flake <uri>` or similar
   - [ ] Or manually removes `/nix/store` entries for the flake (if safe)
   - [ ] Logs refresh action with timestamp and user

3. **UI Integration**:
   - [ ] Flakes list view shows "Refresh" button for each flake
   - [ ] Button only visible to Maintainer/Admin roles
   - [ ] Confirmation dialog warns about impact (brief downtime for that flake)
   - [ ] Success/error toast notification after refresh

4. **Safety**:
   - [ ] Action cannot be triggered while evaluation is in progress
   - [ ] Or, cancels in-progress evaluation before refreshing
   - [ ] Does not affect other flakes

---

## Technical Approach

### Option 1: Use Nix's Built-in Refresh
```rust
tokio::process::Command::new("nix")
    .args(&["flake", "update", "--refresh", flake_uri])
    .output()
    .await?;
```

This tells Nix to ignore cached git fetches and re-clone from the remote.

### Option 2: Clear Nix Flake Cache Directory
Nix stores flake lock files and git clones in `~/.cache/nix/flake-registry` or similar. We could:
1. Identify the cache directory for the flake
2. Remove it
3. Trigger a fresh evaluation

**Risks**: Need to ensure we don't break concurrent evaluations or corrupt Nix's cache.

### Option 3: Git-level Refresh in Server's Workspace
If the server maintains its own git clones (not just relying on Nix), we could:
```bash
cd /path/to/flake/clone
git fetch --all --prune
git reset --hard origin/main
```

But this only applies if we're managing clones separately from Nix.

### Recommended Approach
Start with **Option 1** - use Nix's `--refresh` flag. This is the safest and most idiomatic approach.

---

## Architectural Constraints

- Must not break ongoing evaluations for other flakes
- Must be RBAC-protected (Maintainer/Admin only)
- Should be auditable (log who triggered refresh and when)
- Should be idempotent (safe to run multiple times)

---

## Impact Areas

**Packages Modified**:
- `packages/default/src/handlers/api/flakes.rs` (add refresh endpoint)
- `packages/default/src/queries/flakes.rs` (add refresh action, if needed)
- `packages/default/src/flake/eval.rs` (add refresh_flake_cache function)
- `packages/web-ui/src/views/flakes_list.rs` (add Refresh button)
- `packages/web-ui/src/api/client.rs` (add refresh_flake API call)

**Database**: 
- Optional: Add `flake_refreshes` audit table to track refresh events

**Testing**:
- Create scenario where force-push breaks evaluation
- Trigger refresh
- Verify flake can be evaluated successfully after refresh

---

## Dependencies

**Blocked by**: 
- TASK-209 (helpful to see error messages first, to confirm root cause)

**Blocks**: None

---

## Verification Plan

### Manual Test Scenario

1. **Setup**:
   - Create a test flake repository
   - Add it to Crystal Forge
   - Verify it evaluates successfully

2. **Simulate Force-Push**:
   - Push a commit to the flake repo
   - Wait for Crystal Forge to evaluate it
   - Force-push to rewrite history (remove that commit)
   - Wait for next evaluation attempt
   - **Expected**: Evaluation fails with git-related error

3. **Test Refresh**:
   - Click "Refresh Flake" in UI
   - Wait for refresh to complete
   - Trigger new evaluation (or wait for next automatic evaluation)
   - **Expected**: Flake evaluates successfully with latest commit

### Integration Test
- Add test to `checks/web-ui` that simulates this scenario (if feasible)
- Or add manual QA test checklist

---

## Out of Scope

If during implementation you discover:
- Need for automatic retry with refresh → create separate task
- Need to refresh all flakes at once → create separate task
- Need for more sophisticated cache management → create separate task

---

## Security Considerations

- **RBAC**: Only Maintainer/Admin can refresh (prevents abuse)
- **Rate limiting**: Consider adding rate limit (e.g., 1 refresh per flake per 5 minutes)
- **Audit logging**: Log who triggered refresh and when (for compliance/debugging)

---

## Notes

- This is a recovery mechanism, not a fix for the root cause (force-pushes)
- Should be documented in user guide as solution for "stuck flakes"
- May want to add this to health check recommendations when evaluation errors persist

---

## Related Tasks

- TASK-209: Expose flake evaluation error messages (prerequisite for diagnosing root cause)
- TASK-160: Create Eval queue view (related to evaluation management)
