---
id: TASK-221
title: Add flake credential management and convert system add/edit to modal UI
status: In Progress
assignee: []
created_date: '2026-03-28 22:14'
updated_date: '2026-03-29 19:15'
labels:
  - backend
  - frontend
  - database
  - security
  - credentials
  - nix-integration
  - modal-ui
  - enhancement
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

Flakes requiring authentication (private GitLab repos, private GitHub repos, etc.) cannot currently be used by Crystal Forge because there's no way to provide credentials for Nix to access them. Additionally, the current inline system add/edit UI is inconsistent with the rest of the application which uses modal dialogs for entity management. Systems are also coupled to their hostname for NixOS configuration name lookup, with no way to decouple them.

When users try to add a flake that requires authentication, Nix operations (evaluation, building) fail with authentication errors. Users have no way to provide:
- Personal Access Tokens (PAT) for HTTPS access
- SSH keys for git+ssh access  
- Username/password for basic auth
- Other credential types Nix supports (netrc, etc.)

The inline system management UI also doesn't match the established modal pattern used for flakes, environments, builders, etc., creating UX inconsistency.

## Goal

1. **Flake Credential Management:**
   - Add ability to store and manage credentials per flake
   - Support multiple authentication methods (PAT, SSH keys, username/password)
   - Securely pass credentials to Nix when evaluating/building flakes
   - Integrate credential configuration into the flake edit/config modal

2. **System Configuration Name Decoupling (absorbs TASK-214):**
   - Add `system_configuration_name` field to the systems table
   - Decouple the system hostname from the NixOS configuration name in the flake
   - Migration backfills existing systems with hostname as default config name
   - Build/evaluation logic uses config name field for flake attribute path construction

3. **System Configuration Scope Control:**
   - Allow users to choose whether to build ALL nixosConfigurations from a flake or only specific ones tied to CF systems
   - Make the nixosConfiguration name definable when adding a system
   - Allow editing this configuration after system creation

4. **Modal-based System UI:**
   - Convert inline system add/edit to modal dialog (consistent with other entity management)
   - Support editing system properties after creation
   - Include systemConfiguration name field

## Non-Goals

- Implementing credential rotation or expiry policies (future enhancement)
- Auto-detecting authentication requirements (Nix errors are sufficient signal)
- Supporting all possible Nix authentication mechanisms in v1 (start with PAT, SSH, username/password)
- Modifying flake evaluation caching strategy
- Changing how Nix itself handles credentials (we're just providing them)
- Auto-detecting available nixosConfigurations from flakes (could be future enhancement)

## Scope

### 1. Backend: Flake Credentials
- Database schema for storing encrypted flake credentials
- API endpoints for CRUD operations on flake credentials
- Secure credential storage (encryption at rest)
- Credential injection mechanism for Nix operations (env vars, netrc, SSH agent, etc.)
- Support for: PAT (HTTPS), SSH keys, username/password

### 2. Backend: System Configuration Name (from TASK-214)
- Add `system_configuration_name` VARCHAR field to systems table (nullable, defaults to hostname)
- Migration to backfill existing systems with hostname as config name
- Update system creation/edit API to accept and validate the field
- Update build queue and deployment logic to use config name for flake attribute path construction
- Current: `flake#nixosConfigurations.{hostname}` → New: `flake#nixosConfigurations.{config_name}`

### 3. Backend: System Configuration Scope
- Add `build_scope` field to flakes table: `all_configs` | `cf_systems_only`
- Use this field when determining which nixosConfigurations to evaluate/build
- Use the `system_configuration_name` field for proper mapping

### 4. Frontend: Flake Credential UI
- Add "Credentials" section to flake edit/config modal
- UI for selecting credential type (PAT, SSH, username/password)
- Secure input fields (password-type) for sensitive values
- Validation and testing of credentials (optional: test connection button)

### 5. Frontend: Build Scope UI
- Add build scope selector in flake modal: "Build all configurations" vs "Build only CF-managed systems"
- Help text explaining the difference

### 6. Frontend: System Modal UI
- Create new SystemModal component (similar to FlakeModal, EnvironmentModal, etc.)
- Replace inline add/edit UI with modal trigger buttons
- Include systemConfiguration name field with help text
- Support edit mode (populate existing system data)

## Architectural Constraints

- Credentials MUST be encrypted at rest in the database (use server's encryption key)
- Credentials MUST NOT be logged or exposed in API responses (except masked versions)
- SSH private keys should be stored securely and made available to Nix via SSH agent or direct file injection
- Nix credential mechanisms to support:
  - PAT: via netrc or URL embedding (e.g., `https://oauth2:TOKEN@gitlab.com/...`)
  - SSH: via SSH agent or config
  - Username/password: via netrc
- Follow established modal component patterns in web-ui
- Maintain separation between credential management and flake configuration
- Build scope should default to `cf_systems_only` to avoid unnecessary builds
- `system_configuration_name` NULL means "use hostname" — backward compatible
- Hostname remains the primary system identifier

## Verification Plan

### Flake Credentials
1. Add a private GitLab flake without credentials → verify evaluation fails
2. Add PAT credentials for the flake → verify evaluation succeeds
3. Add a flake with SSH URL, configure SSH key → verify evaluation succeeds
4. Add username/password credentials → verify they're encrypted in DB
5. Verify credentials are NOT exposed in API responses (should show masked value or type only)

### System Configuration Name
1. Create a system with hostname "prod-web-01" using config name "webserver" → verify build uses `flake#nixosConfigurations.webserver`
2. Create a system without config name → verify defaults to hostname
3. Verify existing systems continue to work after migration (backfilled with hostname)
4. Edit system's config name → verify subsequent builds use new name

### Build Scope
1. Create flake with multiple nixosConfigurations, set scope to "all" → verify all are evaluated
2. Change scope to "cf_systems_only" → verify only systems with matching CF system records are built

### System Modal UI
1. Click "Add System" → modal opens with empty form
2. Fill form including systemConfiguration name → verify system created
3. Click edit on existing system → modal opens with populated data
4. Modify system configuration name → verify update succeeds
5. Verify modal follows same UX pattern as flake/environment/builder modals

### Security
1. Check database directly → verify credentials are encrypted
2. Check API responses → verify no plaintext credentials
3. Check logs → verify credentials not logged

## Impact Areas

**Backend:**
- `packages/server/src/db/schema.sql` - new `flake_credentials` table, `build_scope` on flakes, `system_configuration_name` on systems
- `packages/server/src/db/models/` - FlakeCredential model, updates to Flake and System models
- `packages/server/src/api/flakes.rs` - credential CRUD endpoints
- `packages/server/src/api/systems.rs` - system creation/update with config name
- `packages/server/src/services/nix/` - credential injection into Nix operations
- `packages/server/src/services/crypto.rs` (or new) - encryption/decryption utilities
- Build job creation logic - use config name for flake attribute path
- Evaluation logic - use build_scope to filter configurations

**Frontend:**
- `packages/web-ui/src/views/flakes.rs` - add credential UI to modal
- `packages/web-ui/src/views/systems.rs` - replace inline UI with modal
- `packages/web-ui/src/components/` - new SystemModal component
- `packages/web-ui/src/api/models.rs` - FlakeCredential, System updates
- `packages/web-ui/src/api/client.rs` - credential and system API calls

## Risk Level

**High** - Involves security-sensitive credential storage, database schema changes, and touches core Nix integration flow

### Mitigation
- Use industry-standard encryption (AES-256-GCM or similar)
- Audit all code paths that touch credentials
- Implement credential masking in all outputs
- Use Nix's native credential mechanisms (don't reinvent)
- Migration is backward-compatible (NULL config name = use hostname)

## Dependencies

- None (TASK-214 has been absorbed into this task)
- Requires encryption key management setup in server configuration
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Flake credentials table exists with encrypted storage for PAT, SSH keys, and username/password
- [ ] #2 API endpoints exist for creating, reading, updating, and deleting flake credentials
- [ ] #3 Credentials are encrypted at rest using strong encryption (AES-256-GCM or equivalent)
- [ ] #4 API responses never expose plaintext credentials (show credential type or masked value only)
- [ ] #5 Flake edit/config modal includes Credentials section with UI for all supported auth types
- [ ] #6 Nix evaluation and build operations successfully use provided credentials (PAT, SSH, username/password)
- [ ] #7 Flakes table has build_scope field (all_configs | cf_systems_only) with default cf_systems_only
- [ ] #8 Flake modal includes build scope selector with explanatory help text
- [ ] #9 Build/evaluation logic respects build_scope setting (only processes selected configurations)
- [ ] #10 Systems table has system_configuration_name field with migration backfilling hostname for existing systems
- [ ] #11 System creation API accepts optional system_configuration_name (defaults to hostname if null)
- [ ] #12 Build jobs use system_configuration_name to construct flake attribute paths (fallback to hostname)
- [ ] #13 System add/edit UI converted from inline to modal dialog component
- [ ] #14 SystemModal includes systemConfiguration name field with help text explaining its purpose
- [ ] #15 Existing systems can be edited via modal (click edit button opens modal with current values)
- [ ] #16 SystemModal follows same UX patterns as FlakeModal, EnvironmentModal, BuilderModal
- [ ] #17 Private flake with PAT credentials successfully evaluates and builds
- [ ] #18 Private flake with SSH key credentials successfully evaluates and builds
- [ ] #19 Credentials are not exposed in server logs or error messages
- [ ] #20 Database inspection confirms credentials are encrypted (not plaintext)
- [ ] #21 Existing systems continue to work unchanged after migration (backward compatible)
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: agent-claude on gray in ~/code/crystal-forge/TASK-221-flake-credentials-modal-ui

MR: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/194

Reviewer feedback addressed in second commit da15a560: (1) update_system_handler now returns 400 for unknown env/flake name instead of silently NULLing; (2) get_system_detail_by_id fixed to join systems table for system_configuration_name; (3) 13 DB-backed integration tests added and all pass; (4) cargo sqlx prepare re-run; (5) AddFlakeForm converted to modal; (6) EditFlakeDialog widened; (7) Generate button themed; (8) config-name placeholder mirrors hostname.

LOCK takeover approved by maintainer: opencode-gpt-5.3-codex on reckless in /home/mcamp/code/crystal-forge/TASK-221-flake-credentials-modal-ui

Emergency follow-up: investigate and patch `/flakes` tab crash reproducible on TASK-221 branch (dev does not reproduce).

Emergency crash mitigation pushed in commit 367b46de on MR194: cap per-commit stored systems to 120, cap rendered chips to 24, truncate oversized chip labels (96 chars), and disable eval websocket stream for commits with >80 systems to prevent browser tab crashes on /flakes under high-payload scenarios.
<!-- SECTION:NOTES:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 Database migration adds flake_credentials table and build_scope field
- [ ] #2 Encryption utilities implemented and tested
- [ ] #3 API endpoints documented (OpenAPI/Swagger or equivalent)
- [ ] #4 Security audit of credential handling completed
- [ ] #5 Frontend modal components follow accessibility standards (keyboard nav, ARIA labels)
- [ ] #6 Error messages for credential failures are user-friendly and don't leak sensitive info
- [ ] #7 Integration tests cover all credential types and build scope scenarios
- [ ] #8 Manual testing completed with real private repositories (GitLab, GitHub)
- [ ] #9 Code review completed with focus on security
- [ ] #10 Documentation updated explaining how to configure flake credentials
<!-- DOD:END -->
