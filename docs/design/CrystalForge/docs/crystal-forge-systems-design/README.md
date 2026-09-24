# Systems design and System Detail CVEs handoff

The complete `systems-view-design-v0.1.md` file contains document version **0.3**.
The filename remains unchanged for stable repository links. The SC1 contract,
prompt, and manual guide are handoff revision 2.

## Authority

The owner's Claude design implementation is the UI source of truth. The Systems
architecture defines data and behavior, not replacement UI. Start with the UI
authority rule in Section 1.0, then Section 22 and `system-cves-chunk-1.md`.

Before browser-visible edits, map the affected states to existing Claude design
components/states. Do not invent banners, badges, panels, fields, controls or
workflows. A missing design state returns to the owner's Claude design workflow
and blocks its UI acceptance. It does not block independent backend work.

## Scope

SC1 corrects target, scan selection and trusted reconciliation of a known
external activation. Unmapped Current stays unmapped. A uniquely mapped
running derivation can supply its schema-1 scan provisionally read-only while
retained proof is missing. Server-owned ingestion or bounded repair may bind
the observed generation to a real certified artifact with external provenance;
normal Current triage then applies. Current follows observed activation changes
on refresh. Explicit targets remain read-only and exact. A new evaluation adds
a browsing choice; it does not change the running configuration.

Header/count consolidation remains SC2. New host/environment triage controls
and cross-generation continuity remain later slices; SC1 reuses the existing
exact Current triage pipeline after reconciliation. The working completion
mapping is not approval for a new UI badge, status widget or automatic closure.

## Evidence and contents

Original application audit: `58006084aa699b84bcb1d02d6f911d4d4ee94ea3`.
Recorded decision-update inspection: `327d03b6d58055eb688fe657f12e223b8419f446`.
This correction does not recheck the current repository head or design state.
It changes the supplied documents, not application code or repository state.

The bundle contains complete files: the Systems document, SC1 contract, prompt,
manual guide, 12 matching Mermaid sources, prior screenshot crops, and a review
manifest. The screenshots are historical evidence, not UI approval. The diagrams
model data and state; they are not mockups. The 36-case broad regression matrix
is retained. SC1 has 15 cases including the design-authority gate.

No application tests, database operations, browser workflows or new visual audit
were performed for this correction. Artifact checks are recorded separately.
Fleet CVEs, Compliance and the continuity proposal are not overwritten here.
