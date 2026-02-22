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
-->\n\n# Issue Details\n\n- **Issue ID:** 173748587\n- **Issue IID:** 100\n- **Title:** Implement Agent Deployment Capability\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-09-21T19:27:14.05Z\n- **Updated at:** 2025-09-28T19:29:32.162Z\n\n# Description\n\n## Summary

Add bidirectional deployment capabilities to Crystal Forge, enabling the server to instruct agents which NixOS configuration they should be running. This extends the current monitoring-only system to include configuration management.

[**Design Document:**](./docs/deployments_design_doc.md)

## Acceptance Criteria

### Phase 1: Core Deployment Implementation

- [ ] **Database Schema Changes**
  - [ ] Add `desired_derivation`, `deployment_policy`, `server_public_key` columns to `systems` table
  - [ ] Add `crystal_forge_enabled` column to `derivations` table  
  - [ ] Add `cf_deployment` to valid `change_reason` values in `system_states`

- [ ] **Crystal Forge Assertion**
  - [ ] Implement build-time validation using `nix repl` to check `services.crystal-forge.client.enable = true`
  - [ ] Set `derivations.crystal_forge_enabled = true` only when assertion passes
  - [ ] Block deployments for derivations where `crystal_forge_enabled = false`

- [ ] **Server-Side Implementation**
  - [ ] Generate server Ed25519 keypair for deployment signing
  - [ ] Extend agent POST response to include deployment instructions when `desired_derivation` is set
  - [ ] Implement deployment command signing with server private key
  - [ ] Add logic to determine deployment instructions based on `deployment_policy`

- [ ] **Agent-Side Implementation**
  - [ ] Add server public key configuration to agent
  - [ ] Implement deployment command signature verification
  - [ ] Add deployment execution logic using `nixos-rebuild switch` equivalent
  - [ ] Report deployment results via `change_reason = 'cf_deployment'` in next state POST

- [ ] **Cache Integration**
  - [ ] Require derivations to have `cache-pushed` status before deployment
  - [ ] Include cache URL in deployment response
  - [ ] Add configurable fallback to local builds

- [ ] **Manual Deployment Policy**
  - [ ] Implement `manual` deployment policy (admin sets `desired_derivation` explicitly)
  - [ ] Default new systems to `deployment_policy = 'manual'`

## Technical Implementation Notes

### Database Migration
```sql
-- Add to systems table
ALTER TABLE systems 
  ADD COLUMN desired_derivation TEXT,
  ADD COLUMN deployment_policy TEXT DEFAULT 'manual' 
    CHECK (deployment_policy IN ('manual', 'auto_latest', 'pinned')),
  ADD COLUMN server_public_key TEXT;

-- Add to derivations table  
ALTER TABLE derivations 
  ADD COLUMN crystal_forge_enabled BOOLEAN DEFAULT FALSE;

-- Update system_states check constraint
-- Add 'cf_deployment' to existing: 'startup', 'config_change', 'state_delta'
```

### API Response Format
```json
{
  "status": "ok",
  "deployment": {
    "derivation_path": "/nix/store/abc123-nixos-system-hostname",
    "signature": "base64-encoded-ed25519-signature",
    "cache_url": "https://cache.company.com",
    "timestamp": "2025-09-21T10:30:00Z"
  }
}
```

### Configuration Requirements
- Binary cache must be configured and accessible
- Server signing key must be distributed to agents
- Agents must have sufficient privileges for `nixos-rebuild`

## Testing Requirements

- [ ] **Unit Tests**
  - [ ] Crystal Forge assertion validation
  - [ ] Deployment command signing/verification
  - [ ] Deployment policy logic

- [ ] **Integration Tests**
  - [ ] End-to-end deployment flow
  - [ ] Cache integration
  - [ ] Agent deployment execution
  - [ ] Deployment failure handling

- [ ] **Security Tests**
  - [ ] Signature verification prevents tampered commands
  - [ ] Crystal Forge assertion blocks agent-disabling configs
  - [ ] Invalid signatures are rejected

## Documentation Updates

- [ ] Update agent configuration documentation for server public key
- [ ] Document deployment policy options
- [ ] Add troubleshooting guide for deployment failures
- [ ] Update API documentation for enhanced POST response

## Future Enhancements (Not in scope)

- Group-based deployment policies
- `auto_latest` and `pinned` deployment policies  
- Deployment scheduling
- Automatic rollback on failure
- Advanced deployment monitoring\n\n# Assignees\n\nMatt\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n