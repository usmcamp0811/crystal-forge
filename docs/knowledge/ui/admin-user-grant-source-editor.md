---
type: UI Design
title: Admin user grant source editor
description: Maintainer-approved MR 345 review addendum for explicit local grant edits, separate effective and source authority, and legacy replacement acknowledgment.
tags:
  - crystal-forge
  - ui
  - admin
  - authorization
  - oidc
implementation_status: partial
sources:
  - id: prototype
    resource: "Crystal Forge repository file docs/design/CrystalForge/crystal-forge.html at commit 79dcc4d8"
    title: Readable single-HTML prototype used for design comparison
  - id: source-model
    resource: "Crystal Forge repository file packages/default/crates/cf-server/migrations/0335_oidc_entitlement_sources.sql at commit c2c22b02"
    title: Independent local, IdP and unresolved legacy grant contributions
---

# Admin user grant source editor

## Authority and implementation state

This is the maintainer-approved review addendum requested for MR !345,
TASK-529.2. It is authoritative for this source-ownership fix where the
single-HTML prototype has merged role/environment fields. Do not restore those
merged fields for visual parity. The prototype does not model source ownership.
Approval of this bounded contract is not browser acceptance or full M0 completion.

Current uncommitted implementation references are
`packages/web-ui/src/views/admin.rs` (`EditUserModal`, `LocalGrantEdit`),
`packages/web-ui/src/api/models.rs`, and the server files named by the
[PATCH contract](../api/builders-queues-environments-dashboard-admin-api.md#user-patch-contract-mr-345-requested-changes).
The provenance above identifies committed background only, not these new fixes.
The reported UI farm result is 887 tests, including nine new tests. Current-source
server, PostgreSQL and new real-browser verification remain **PENDING**. The
full PAT admission gate is not wired; `implementation_status` remains `partial`.

## Approved editor contract

- Keep the current Role-then-Environments layout, account status/password
  controls, modal shell and Admin tabs. Keep Profile & Preferences and its
  self-service boundary separate. This fix does not reorganize those surfaces.
- In **each** grant category, show effective authority read-only and show all
  three sources separately: Local, Identity provider, and Unresolved legacy.
  Role sources use `local_roles`, `oidc_roles`, `legacy_roles`; environment
  sources use `local_environments`, `oidc_environments`, `legacy_environments`.
  Show empty values as None. Login source is a separate account attribute, not
  evidence of ownership. Effective empty environments must not display `all`.
- Initialize editable values only from local source metadata. Require separate
  opt-in for role and environment edits. Unchanged opt-ins, ordinary no-op save,
  status-only and password-only edits MUST NOT send grant categories or set grant
  intent. Never round-trip the effective union into local replacement fields.
- A changed local role sends `role` with `edit_local_grants: true`. “No local
  role” sends `clear_local_role: true` when removing an existing local override;
  it preserves IdP and unresolved legacy roles. Never send role and clear together.
- Replacing a role with unresolved legacy roles requires explicit acknowledgment
  that replacement removes those legacy contributions and creates a local role.
  Local-only role clear needs no legacy classification acknowledgment.
- Environment replacement removes prior local **and legacy** memberships, then
  installs the local selection, preserving IdP memberships. With legacy entries,
  require explicit acknowledgment, including for an empty selection. An unchanged
  selection plus acknowledged legacy classification is deliberate replacement;
  simply opening or opting into the editor is not.
- Missing/null `grant_sources` means ownership is unknown. Fail closed for grant
  controls and explain why. Status and password edits remain available. Do not
  infer ownership from effective grants or `identity_source`.
- During save, disable grant mutations and duplicate submission. On validation,
  last-Admin, network or server error, keep the modal open, show the error and
  preserve the form draft for correction. Close/reset only after successful save
  or explicit dismissal. Success must display the returned effective union and
  source metadata, not assume the requested local role is the effective role.
- Keep source metadata free of raw claims, issuer/subject, identity IDs, tokens,
  credentials and password hashes. Ownership labels are not secret inspection.

## Comparison with the single-HTML design

The comparison is against `AdminUsers` / `UserModal` in
`docs/design/CrystalForge/crystal-forge.html`, not an executed visual test.

| Review point | Prototype and approved disposition |
| --- | --- |
| Missing | The prototype has no source-aware opt-ins, local-only role clear, legacy acknowledgments or unknown-metadata state. Add these controls within the existing modal. |
| Reordered | Retain Role before Environments and retain surrounding Admin/Profile navigation. Do not reorder tabs or move self-service into Admin to accommodate this fix. Source summaries precede each category's local controls. |
| Merged | The prototype initializes one `role` and one `envs` value from the user and describes a manual override. Replace that ambiguity with read-only effective values plus independent local editors; never merge IdP/legacy contributions into their selections. |
| Metadata | The prototype's single `source`/OIDC-groups display cannot describe overlapping ownership. Use six source vectors, separate from login origin; no raw provider evidence is needed. |
| Behavior | Prototype Save closes the modal without a server mutation model. The fix must send only intentional categories, preserve error drafts, acknowledge legacy replacement and respect atomic PATCH/last-Admin rejection. Its `all` pill is not an empty-membership encoding. |
| Visual hierarchy | Retain existing modal, fields, chips, help/callout styles and primary Save/secondary Cancel hierarchy. Effective/source summaries are read-only context; local controls are explicitly labeled actions; legacy consequences appear adjacent to their acknowledgment. Source labels must remain legible and distinct rather than collapse into one “Role” or “Source” badge. |

The source comparison does not prove rendered spacing, responsive behavior,
focus/keyboard behavior or error-state screenshots. Parent verification must
exercise the new `admin-source-grants` browser fragment in the existing
`integration-access` `lifecycle-local-bootstrap` scenario and retain design
comparison images. Existing UI unit results do not substitute for that run.

## Related concepts

- [Admin console and login views](admin-console-and-login-views.md)
- [OIDC entitlement ownership and freshness](../security/oidc-entitlement-ownership.md)
- [Design system: component and interaction patterns](design-system-components-and-interaction.md)
