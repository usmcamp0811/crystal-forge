# Crystal Forge CVEs View design review

**Version:** 0.1, review draft  
**Date:** 2026-09-23  
**Audited revision:** `58006084aa699b84bcb1d02d6f911d4d4ee94ea3`

## Main document

[Read the fleet CVEs architecture and consistency document](cves-view-design-v0.1.md).

This draft covers the fleet `/cves` page, grouped and flat inventory, filters,
statistics, the inventory drawer, environment triage, host overrides, scan
admission, and the exact-CVE POA&M lifecycle. It keeps current code, existing
specifications, and proposed behavior separate.
Its AS-BUILT evidence and Mermaid sources remain pinned to the audited SHA.
The [CVE/POA&M continuity design, Section 29](../../cve-poam-evidence-continuity-design-spec.md#29-acceptance-criteria)
now supersedes the audited retained-artifact CVE gate, frozen verification
lineage, and membership equality. TASK-326.2.2 is in progress; this audit did
not verify its implementation. Config and rollback authority remains separate.

## Review order

Start with Section 2 for existing specifications and conflicts. Sections 7–8
explain source selection and count units. Sections 10–14 explain refresh,
host overrides, transactions, and the current verification rule. Section 15
compares the existing continuity proposal. Sections 20 and 25 contain the gap
and decision registers. Section 23 contains 56 regression scenarios.

The baseline-generation verification rule and read-only Current inventory
describe the audited source, not the later CVE contract. Dynamic environment
membership and cross-revision verification are required by the continuity
design, not established as implemented by this audit.

## Bundle contents

- `cves-view-design-v0.1.md`: complete review document, with inline Mermaid source.
- `diagrams/`: 12 matching editable Mermaid files and a diagram manifest.
- `source-manifest.json`: commit-pinned source references.
- `verification.md`: artifact checks and limits of execution evidence.

The bundle contains no application changes or fix prompt. No fleet screenshot
was supplied or produced. Existing Systems/Scanning screenshots are not reused
as evidence for the fleet CVEs page. No application tests or database queries
were executed for this draft.

## Diagrams

| File | Meaning | Status |
|---|---|---|
| [01-surface-composition](diagrams/01-surface-composition.mmd) | As-built surface composition | AS-BUILT |
| [02-persistence-relationships](diagrams/02-persistence-relationships.mmd) | Conceptual persistence relationships | AS-BUILT |
| [03-producer-read-data-flow](diagrams/03-producer-read-data-flow.mmd) | Producer and fleet read dependencies | AS-BUILT |
| [04-inventory-selection](diagrams/04-inventory-selection.mmd) | Current, scheduled and Historical selection | AS-BUILT |
| [05-count-overlap](diagrams/05-count-overlap.mmd) | Illustrative host count overlap | EXPLANATORY |
| [06-page-request-and-refresh](diagrams/06-page-request-and-refresh.mmd) | Independent requests and missing parent invalidation | AS-BUILT |
| [07-drawer-state-machine](diagrams/07-drawer-state-machine.mmd) | Drawer and nested editor states | AS-BUILT |
| [08-host-override-precedence](diagrams/08-host-override-precedence.mmd) | Effective host decision versus fleet rollup | AS-BUILT |
| [09-triage-transaction](diagrams/09-triage-transaction.mmd) | Triage transaction and authority revalidation | AS-BUILT |
| [10-current-verification-rule](diagrams/10-current-verification-rule.mmd) | Baseline-bound verification predicate | AS-BUILT |
| [11-proposed-evidence-continuity](diagrams/11-proposed-evidence-continuity.mmd) | Immutable baseline and moving Current evidence | EXISTING PROPOSAL |
| [12-proposed-invalidation](diagrams/12-proposed-invalidation.mmd) | Shared read-model invalidation contract | PROPOSED |

Mermaid source is embedded in the Markdown and also stored separately. The
files can be edited without changing application source. Compilation/rendered
layout verification is reported in `verification.md`, not inferred from the
presence of a diagram file.
