---
id: TASK-365
title: 'STIG import: parse XCCDF .xml → create policies + compliance bundle'
status: To Do
assignee: []
created_date: '2026-06-22 02:54'
labels:
  - compliance
  - api-integration
  - web-ui
  - stig
milestone: m-20
dependencies: []
priority: medium
ordinal: 317000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

The Compliance page has an "Import STIG" button and a stub modal (added in TASK-334 / MR !285) that is clearly marked "not yet implemented". Users cannot actually upload a DISA XCCDF benchmark file today — the UI is a preview only.

## Goal

Wire the Import STIG modal end-to-end: the user uploads a DISA XCCDF `.xml` file in the browser, Crystal Forge parses it server-side (or client-side via WASM), generates one `DeploymentPolicy` per STIG rule, and creates a `ComplianceBundle` grouping them. The result is immediately visible in the bundle catalog.

## Non-Goals

- Automated agent-side evaluation of STIG rules against live systems (that is TASK-320 / evaluator epic).
- SCAP datastream or OVAL format support (XCCDF only for this task).
- UI for editing individual imported rules post-import (handled via existing Edit bundle / policy flows).

## Scope

### Backend

1. **Upload endpoint** `POST /api/v1/compliance/bundles/import-stig`
   - Accepts `multipart/form-data` with fields: `file` (XCCDF XML), `bundle_name` (string), `env_ids` (repeated UUID), `rule_ids` (optional repeated string — selected subset).
   - Streams the XML body; rejects files > 10 MB.
   - Parses the XCCDF using a Rust XML parser (quick-xml or roxmltree) to extract:
     - Benchmark `<title>`, `<version>`, and plain-text `Release` field.
     - All `<Rule>` elements: `id`, `severity`, `<title>`, `<version>` (STIG ID), `<fixtext>`, `<check-content>`, `<ident>` (SRG).
   - Creates one `deployment_policy` row per selected rule (upsert on a stable slug key `stig-<stigId-slug>` to avoid duplicates across re-imports).
   - Creates one `compliance_bundle` row linking all policy IDs to the specified environments.
   - Returns the created bundle as `ComplianceBundleSummary` JSON.
   - Requires admin RBAC (same as `POST /api/v1/compliance/bundles`).

2. **Migration** — no new tables required; reuses `compliance_bundles`, `compliance_bundle_policies`, and `deployment_policies`. Add a `source_stig_id` nullable text column to `deployment_policies` if not already present to track import provenance.

3. **Unit tests** — parse a small sample XCCDF fixture (5 rules); assert correct policy count, severity mapping, and bundle name.

### Frontend

4. **Wire the upload modal** (`ImportStigModal` in `compliance.rs`):
   - Step 1 **Upload**: real file `<input type="file" accept=".xml">` + drag-and-drop. On file select, POST the file to the backend immediately for parsing metadata (or parse in WASM if preferred — see decision note below). Show a spinner while parsing.
   - Step 2 **Review**: bundle name field (pre-filled from benchmark title), environment badge toggles (real `environments` signal), scrollable rule checklist with CAT I/II/III severity toggles, summary callout.
   - Step 3 **Done**: success stats (new policies / reused / total), link to view the bundle.
   - Error handling: XML parse errors, duplicate bundle names, file-too-large.

5. **Remove the "not yet implemented" callout** and the disabled state from the stub.

## Decision Note — parse location

**Option A (recommended):** Parse XCCDF in WASM on the client (avoids a file upload round-trip for large XMLs; the design reference does this). Send only the extracted rule list + bundle metadata to the backend `POST` endpoint.

**Option B:** Stream the raw XML to the server and parse there. Simpler Rust code; avoids shipping an XML parser in WASM.

Choose whichever fits the existing architecture better. Document the choice in code comments.

## Architectural Constraints

- Reuse `ComplianceBundleSummary` DTO returned by existing `GET /api/v1/compliance/bundles`.
- Backend parser lives in `packages/default/src/stig/` or similar domain module — not in the handler.
- No business logic in the UI; the review step sends structured data to the API.
- Keep the upload endpoint idempotent on the rule slug so re-importing the same STIG version does not create duplicate policies.

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 User can drag-and-drop or browse for a DISA XCCDF .xml file in the Import STIG modal
- [ ] #2 The file is parsed (client or server) and the review step shows benchmark title, version, and a checklist of rules with CAT I/II/III severity badges
- [ ] #3 User can toggle individual rules and use CAT bulk-toggle buttons; selected count shown live
- [ ] #4 User selects environments (real env list) and confirms a bundle name
- [ ] #5 Clicking 'Create bundle + N policies' POSTs to the backend; spinner shown during request
- [ ] #6 On success the Done step shows counts (new policies / reused / total) and a 'View bundle' button that selects the new bundle in the catalog
- [ ] #7 New bundle and policies appear in the catalog and policy list without a full page reload
- [ ] #8 Re-importing the same STIG file does not create duplicate policies (upsert by slug)
- [ ] #9 Uploading a non-XCCDF file shows a clear error message
- [ ] #10 The 'not yet implemented' callout and disabled drop zone are removed
- [ ] #11 cargo test passes for XCCDF parser unit tests
- [ ] #12 nix build .#checks.x86_64-linux.web-ui passes including modal screenshot

## Verification Plan

- `nix develop -c cargo fmt -- --check`
- `nix develop -c cargo clippy -- -D warnings`
- `nix develop -c cargo check --manifest-path packages/default/Cargo.toml --all-targets`
- `nix develop -c cargo check --manifest-path packages/web-ui/Cargo.toml --target wasm32-unknown-unknown`
- `nix develop -c cargo test --manifest-path packages/default/Cargo.toml -- stig` (XCCDF parser tests)
- `nix build .#checks.x86_64-linux.web-ui` (modal screenshot + integration assertions)

## Impact Areas

- `packages/default/src/handlers/api/compliance.rs` — new import endpoint
- `packages/default/src/stig/` (new module) — XCCDF parser
- `packages/default/src/queries/compliance.rs` — upsert policy + create bundle helpers
- `packages/default/migrations/` — `source_stig_id` column if needed
- `packages/web-ui/src/views/compliance.rs` — wire `ImportStigModal`
- `packages/web-ui/src/api/client.rs` + `models.rs` — import request/response DTOs
- `checks/web-ui/tests/integration-test.js` — screenshot for import modal

## Risk Level

Medium — XML parsing in Rust is straightforward with quick-xml/roxmltree; the main complexity is the multi-step modal state and the upsert logic for deduplicating policies across re-imports.

## Dependencies

- TASK-334 (Done — stub modal already in place; MR !285 merged)
- No other blockers.
<!-- SECTION:DESCRIPTION:END -->
<!-- AC:END -->
