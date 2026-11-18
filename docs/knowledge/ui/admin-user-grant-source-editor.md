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
  intent. The one role exception is an account with several local roles; see
  [Multiple local roles](#multiple-local-roles). Never round-trip the effective
  union into local replacement fields.
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

## Multiple local roles

An account can hold several local roles, for example Admin and Viewer. The
editor (`LocalGrantEdit::request` in `packages/web-ui/src/views/admin.rs`)
handles this case as follows:

- The role selector starts at the highest local role (Admin, then Operator,
  then Viewer). Zero local roles start at "No local role".
- The editor compares the selected replacement with the **full deduplicated
  local role set** from `local_roles`, not only the highest role. Selecting
  Admin is a change when the local set is Admin and Viewer.
- With the role override opted in, an account with several local roles that
  keeps the pre-selected highest role sends `role` and `edit_local_grants:
  true`. The server replaces **all** local role sources with exactly that one
  role. Selecting a different role or "No local role" is also a change and
  follows the normal contract above.
- IdP role contributions are preserved. The replacement also removes
  unresolved legacy role contributions, as the PATCH contract states for any
  `role` replacement. When legacy roles exist, the editor still requires the
  explicit acknowledgment before it sends the role.
- Opt-in alone remains a no-op for an account with zero or one local role.
  The selection equals the local set, so the request omits `role`. The only
  exception is an acknowledged legacy classification, which is deliberate
  replacement.
- The Role section shows a factual notice, `data-testid`
  `multiple-local-roles-notice`, when the account has more than one local
  role. The notice states the number and names of the local roles and that
  saving with the override enabled replaces them with the selected role.
  Accounts with zero or one local role show no notice.

The notice and the comparison are source-level statements. This update does not
claim a rendered or browser result for them. Execution of the related
regressions is recorded in TASK-529.2, not here.

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

The corrected farm Access gate passed at `2c298432`, including the actual browser
password/status PATCH, exact source-row preservation, account-only audit checks
and absence of local fallback after fixture withdrawal. The passing rendered
modal shows IdP Operator/production memberships with empty, disabled local
controls, then preserves those sources on reload. The original Role,
Environments, Password reset and Enabled order and Save/Cancel/Delete hierarchy
remain; only ownership context and intentional local controls are added. This
desktop evidence does not establish a full narrow-screen/keyboard/error-state
matrix or exact-head GitLab CI. The screenshot is retained in the Access
evidence `browser/admin-source-unrelated-save` directory; detailed artifact
identifiers and remaining gates are recorded in TASK-529.2.

## Related concepts

- [Admin console and login views](admin-console-and-login-views.md)
- [OIDC entitlement ownership and freshness](../security/oidc-entitlement-ownership.md)
- [Design system: component and interaction patterns](design-system-components-and-interaction.md)
