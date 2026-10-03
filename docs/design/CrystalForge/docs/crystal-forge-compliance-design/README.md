# Crystal Forge Compliance architecture review, v0.1

Start with [the main design document](compliance-view-design-v0.1.md). Use [the cross-view contract ledger](cross-view-contract-ledger-v0.1.md) when reviewing it with the existing Systems and CVEs drafts.

The main document has 26 sections, 32 open gaps, 80 proposed regression scenarios, 15 pending decisions, and 14 inline Mermaid diagrams. The ledger has 22 open contracts. All three reviews describe the same application source state: the bridge from `58006084` to this document's `931e3622` adds the earlier documents only.

## Contents

| File | Purpose |
|---|---|
| `compliance-view-design-v0.1.md` | Full route, data-source, evidence, POA&M, export, parity, and test audit. |
| `cross-view-contract-ledger-v0.1.md` | Open Systems/CVEs/Compliance review worksheet. |
| `diagrams/*.mmd` | Editable Mermaid sources, identical to the main document's inline diagrams. |
| `gap-register.json` | Source findings and verification priorities. |
| `regression-matrix.json` | Proposed scenarios; all unexecuted in this audit. |
| `decision-register.json` | Pending joint decisions and linked gaps. |
| `cross-view-contract-ledger.json` | Machine-readable copy of the worksheet. |
| `source-manifest.json` | Commit-pinned source references and inspection limits. |
| `diagram-manifest.json` | Diagram inventory and source hashes. |
| `companion-file-record.json` | Hash record for the untouched local companion Markdown files. |
| `verification.md` | What was inspected, not executed, and observed about the MR head. |
| `artifact-checks.json` | Generated-file integrity checks only. |
| `checksums.sha256` | SHA-256 checksums for bundle files other than this checksum file. |

## Rendering and source links

Use a Markdown viewer with Mermaid support for inline diagrams, or edit individual `.mmd` files. No Mermaid renderer or parser was run during this audit. Source references point to the exact reviewed GitLab revision; access requires the normal repository permissions.

## Status

Documentation only. No implementation was changed. No application/database/browser tests were run. No current rendered visual parity, standards-schema validity, or merge readiness is claimed. The prior two documents remain unchanged, and none of the ledger's open decisions is approved here.
