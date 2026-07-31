---
id: TASK-212
title: HOTFIX - Custom policy expressions use wrong variable causing all evaluations to fail
status: Done
created: 2026-03-22
priority: critical
tags: [hotfix, bug, evaluation, policies, blocking]
risk: medium
notes: |
  RESOLVED: Issue was already fixed before task execution
  Investigation date: 2026-03-24
  
  Timeline:
  - March 22 09:52 - First "undefined variable 'config'" errors
  - March 22 19:44 - Last occurrence of error
  - March 23 01:41 - First successful evaluation after fix
  - March 24 - Verified: 33 complete evaluations, 0 pending, system healthy
  
  Root cause: Three custom_check policies used wrong variable prefix
  Resolution: Policies were deleted from database (no custom_check policies exist now)
  Result: Evaluations have been running successfully for 2+ days
  
  No code changes needed - issue resolved via database cleanup.
---

## Problem

All commit evaluations are failing because three custom deployment policies in the database use `config.` instead of `cfg.config.` in their Nix expressions. This causes nix-eval-jobs to fail with "undefined variable 'config'" error.

**Current impact:**
- 0 evaluations completing successfully
- ~15+ commits stuck in pending/failed state
- Eval loop is running but all evaluations fail immediately
- Production system blocked from processing any new commits

**Error from logs:**
```
error: undefined variable 'config'
   at «string»:14:35:
      14|         require_auditd_d86a17b4 = config.services.auditd.enable or false;
        |                                   ^
```

**Affected policies:**
1. `require_auditd_d86a17b4` - uses `config.services.auditd.enable` 
2. `require_firewall_279674da` - uses `config.networking.firewall.enable`
3. `require_ssh_key_auth_90a171ad` - uses `!config.services.openssh.settings.PasswordAuthentication`

All should use `cfg.config.` prefix instead.

**Secondary issue:** Attempting to edit policies via UI fails with "HTTP 400: custom_check policy requires non-empty config.expression" - the UI is clearing the expression field somehow.

## Goal

Unblock all pending evaluations by fixing the three broken custom policy expressions in the database.

## Desired Outcome

- All three policies updated to use `cfg.config.` instead of `config.`
- Pending commits resume evaluation and complete successfully
- UI policy editor fixed to not clear expression field on edit

## Non-Goals

- Comprehensive policy validation (separate task)
- Migration of existing policies (just fix the three broken ones)

## Acceptance Criteria

**Critical (must have):**
- [ ] Three broken policies are corrected in database to use `cfg.config.` prefix
- [ ] At least one pending commit evaluates successfully after fix
- [ ] Server logs show successful evaluations (not just failures)
- [ ] No "undefined variable 'config'" errors in logs

**Important (should have):**
- [ ] UI policy editor allows editing custom policies without clearing expression
- [ ] UI policy editor validates Nix expression syntax before save

**Nice to have:**
- [ ] Validation warning if expression uses `config.` instead of `cfg.config.`
- [ ] Documentation about correct variable scope in policy expressions

## Implementation Plan

### Phase 1: Emergency Database Fix (IMMEDIATE)

1. SSH to reckless production server
2. Connect to postgres as postgres user
3. Run UPDATE queries to fix the three policies:

```sql
-- Fix auditd policy
UPDATE deployment_policies 
SET expression = 'cfg.config.services.auditd.enable or false'
WHERE field_name = 'require_auditd_d86a17b4';

-- Fix firewall policy
UPDATE deployment_policies 
SET expression = 'cfg.config.networking.firewall.enable'
WHERE field_name = 'require_firewall_279674da';

-- Fix SSH key auth policy
UPDATE deployment_policies 
SET expression = '!cfg.config.services.openssh.settings.PasswordAuthentication'
WHERE field_name = 'require_ssh_key_auth_90a171ad';
```

4. Verify fix:
```sql
SELECT field_name, expression 
FROM deployment_policies 
WHERE policy_type = 'custom_check' 
ORDER BY field_name;
```

5. Monitor server logs for successful evaluations:
```bash
journalctl -u crystal-forge-server -f | grep -E "📌 Found|✅ Successfully evaluated|❌ Failed"
```

### Phase 2: Fix UI Policy Editor

**Root cause analysis needed:**
- Why does editing a policy clear the expression field?
- Is this a frontend serialization issue?
- Is this a backend validation issue?

**Likely locations:**
- Frontend: `packages/web-ui/src/components/policies/*.rs`
- Backend: `packages/default/src/api/policies.rs`
- Model: `packages/default/src/models/deployment_policies.rs`

**Investigation steps:**
1. Check UI form handling for custom_check policies
2. Check API endpoint for PUT/PATCH policy updates
3. Check if expression field is being sent in request
4. Check backend validation logic

**Fix approaches:**
- Ensure expression field is properly bound in UI form
- Ensure API endpoint doesn't require expression to be resent if unchanged
- Add better error messages indicating which field is missing

### Phase 3: Add Validation

**Prevent future issues:**
1. Add Nix expression validator (could use `nix-instantiate --parse`)
2. Validate that expressions use `cfg.config.` not `config.`
3. Show validation errors in UI before allowing save
4. Add help text explaining variable scope

## Verification Commands

**Check current policy expressions:**
```bash
ssh reckless 'sudo -u postgres psql crystal_forge -c "SELECT field_name, expression FROM deployment_policies WHERE policy_type = '\''custom_check'\'' ORDER BY field_name;"'
```

**Monitor evaluation progress:**
```bash
ssh reckless 'journalctl -u crystal-forge-server -f | grep -E "📌 Found|✅ Successfully|❌ Failed"'
```

**Check if evaluations are completing:**
```bash
ssh reckless 'sudo -u postgres psql crystal_forge -c "SELECT evaluation_status, COUNT(*) FROM commits GROUP BY evaluation_status;"'
```

## Impact Analysis

**Systems affected:**
- crystal-forge-server on reckless (production)
- All flakes with pending commits
- All deployment pipelines waiting for evaluations

**Blast radius:**
- HIGH: Blocks all automated deployments
- HIGH: Prevents validation of new commits
- MEDIUM: Impacts developer workflow (can't see eval results)
- LOW: Doesn't affect already-deployed systems

## Rollback Plan

If the database fix causes issues:

```sql
-- Rollback to original values
UPDATE deployment_policies 
SET expression = 'config.services.auditd.enable or false'
WHERE field_name = 'require_auditd_d86a17b4';

UPDATE deployment_policies 
SET expression = 'config.networking.firewall.enable'
WHERE field_name = 'require_firewall_279674da';

UPDATE deployment_policies 
SET expression = '!config.services.openssh.settings.PasswordAuthentication'
WHERE field_name = 'require_ssh_key_auth_90a171ad';
```

Alternative: Delete the three policies entirely if they're not critical:
```sql
DELETE FROM deployment_policies 
WHERE field_name IN (
  'require_auditd_d86a17b4',
  'require_firewall_279674da', 
  'require_ssh_key_auth_90a171ad'
);
```

## Files to Modify

**Phase 1 (database fix):** None - direct SQL

**Phase 2 (UI fix):**
- `packages/web-ui/src/components/policies/*.rs` - policy editor form
- `packages/default/src/api/policies.rs` - policy update endpoint
- Possibly `packages/default/src/models/deployment_policies.rs` - validation

**Phase 3 (validation):**
- `packages/default/src/models/deployment_policies.rs` - add validation method
- `packages/default/src/api/policies.rs` - call validation before save
- `packages/web-ui/src/components/policies/*.rs` - display validation errors

## Related Issues

- TASK-211: Timezone configuration (unrelated, can wait)
- Future: Comprehensive policy expression validator
- Future: Policy expression syntax help/documentation

## Risk Assessment

**Risk Level:** Medium
- Database UPDATE is low risk (can be rolled back)
- UI fix is medium risk (need to identify root cause)
- Validation adds complexity but improves UX

**Mitigation:**
- Test SQL UPDATE on a single policy first
- Verify one evaluation succeeds before declaring victory
- Keep rollback SQL ready
- Have database backup before making changes

## Dependencies

None - can be fixed immediately via database UPDATE

## Effort Estimate

**Phase 1 (emergency fix):** 5-10 minutes
**Phase 2 (UI fix):** 1-2 hours (depending on root cause)
**Phase 3 (validation):** 2-4 hours

**Total:** Small to Medium (3-6 hours for complete fix)

## Architectural Constraints

- Must maintain backward compatibility with existing policies
- Expression syntax must remain pure Nix (no DSL)
- Validation must not break valid edge cases
- UI must support multi-line expressions

## Verification Plan

**Tier 0 (immediate verification):**
```bash
# After SQL fix, check one eval completes
ssh reckless 'journalctl -u crystal-forge-server -f'
# Should see: ✅ Successfully evaluated commit <hash>
```

**Tier 1 (after UI fix):**
- Manually edit a policy via UI and save
- Verify expression is preserved
- Verify policy still works in evaluation

**Tier 2 (full integration):**
- Create new custom policy via UI
- Verify it's used in evaluation
- Verify validation catches `config.` error
- Run full evaluation loop and confirm all commits process

## Testing Requirements

**Manual testing:**
- [ ] Edit existing custom policy via UI
- [ ] Create new custom policy with `config.` (should fail validation)
- [ ] Create new custom policy with `cfg.config.` (should succeed)
- [ ] Verify policy is used in next evaluation
- [ ] Verify policy data appears in derivation metadata

**Automated testing (future):**
- Unit test for Nix expression validation
- API test for policy CRUD operations
- Integration test for policy evaluation flow

## Documentation Updates

After fix:
- Document correct variable scope in policy expressions
- Add examples of valid custom policy expressions  
- Document UI policy editor workflow
- Add troubleshooting section for common policy errors
