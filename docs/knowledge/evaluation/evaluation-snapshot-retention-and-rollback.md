---
type: Design Specification
title: "Evaluation snapshot retention, generation rollback, and source reset"
description: "Explains how retained deployment generations keep snapshots, derivations, and commits alive, how generation rollback resolves exact lineage, how branch rewrite and source reset archive commits, and how unavailable content is reported."
tags:
  - crystal-forge
  - evaluation
  - snapshot
  - retention
  - rollback
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Evaluation snapshot retention, generation rollback, and source reset

## Retention

An observed deployment generation records its system, generation, derivation,
commit, store path, and snapshot identity. Restrictive foreign keys keep that
snapshot, derivation, and commit while the generation reference exists. Nix
store garbage collection does not affect the database snapshot.

The exact-CVE evidence, fleet CVE inventory, writer locking, and fleet triage contract that the source also placed in this section is in [exact-cve-evidence-authority-and-inventory-reads.md](../cves/exact-cve-evidence-authority-and-inventory-reads.md) and [exact-cve-writer-locking-and-fleet-triage.md](../cves/exact-cve-writer-locking-and-fleet-triage.md).

Generation rollback accepts a retained generation UUID or system-local
generation number and resolves the exact derivation and store path on the
server. Composite authorization remains constrained to that derivation even if
another commit or duplicate derivation has the same store path. A supplied
legacy store path can only narrow that exact retained lookup. A path by itself, a
foreign system's retained UUID, a failed artifact, or mismatched derivation
lineage does not authorize rollback. Post-migration deployment rows persist the
exact requested derivation ID when the server resolves one. Rollback deployment
creation carries the retained derivation ID unchanged. Ordinary path-only or
legacy deployments can leave this field null, but such a row cannot create
verified generation retention. When multiple same-path deployments exist, an
observation uses the newest deployment issued no later than the observation.
The generation-list response exposes the retained UUID and `rollback_eligible`.
Clients MUST offer rollback only when eligibility is true. `store_path` remains
optional in the rollback request and does not replace retained identity.

Flake timeline snapshots remain attached to their commit records. A branch
rewrite does not reinterpret an old full SHA as a new revision. Rewrite
acceptance archives every old-lineage commit before it removes unretained
history. Restrictive snapshot or generation references can preserve a commit,
but the archived commit is not available through active-revision APIs.
Derivations referenced by retained generations also remain available. A flake
source reset archives commits required by retained generations, exact
deployment-bound derivations, or deployment-bound artifacts and preserves their
derivations. It also archives commits referenced by durable explicit request
reservations and by all deployment rows, including pre-0248 and path-only rows
with no exact artifact or derivation binding. Bounded maintenance later releases
terminal deployment identities after the 24-hour ingestion window. A
derivation-only binding remains authoritative when the evaluation artifact is unavailable.
Source reset removes those
commits from active revision APIs. Generation reads use the retained
identity directly, so the archived snapshot remains queryable without exposing
the old revision as part of the replacement source. Source reset is serialized
with snapshot publication, deployment binding, retention, and reclamation. The
global advisory-lock order is snapshot writer, per-flake sync, then attention.
After the final deployment binding releases, bounded maintenance removes an
otherwise unreferenced archived derivation and commit. A terminal deployment's
commit identity remains protected for the same 24-hour ingestion window. A
durable explicit request reservation protects immutable request intent without a
time limit. Source reset and history rewrite archive every deployment-referenced
commit first. Only bounded maintenance releases an eligible terminal identity;
an `ON DELETE` action never decides eligibility. Other commit snapshots follow
commit timeline retention.

Missing, corrupt, or schema-incompatible content is reported as unavailable.
Migration and successful snapshot finalization recursively validate every
persisted safe-value and provenance variant before setting the artifact's
immutable integrity marker. Option references and content cannot change after
certification. Each Config read checks this marker in the same read-only
`REPEATABLE READ` transaction that selects authoritative first-parent or nearest
preceding usable-generation comparison identity and reads the bounded page.
Scalar variants accept only JSON string, number, Boolean, or null values. Arrays
and objects tagged as scalar are malformed. Malformed content outside the requested page therefore prevents certification
and cannot produce a partially available response. Read cost and response size
remain bounded independently of the complete option corpus.
The server MUST NOT silently re-evaluate during a read to reconstruct it.

## Related concepts

* [Evaluation and Flake Snapshot Architecture](evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot persistence, bounds, reclamation, and redaction](evaluation-snapshot-persistence-bounds-and-redaction.md) - Describes content-addressed snapshot persistence, advisory-lock ordering, hard size bounds, the Stage 2 indexed membership cost, orphan reclamation, and the safe-value and redaction policy applied before persistence.
* [Exact-CVE evidence authority and CVE inventory reads](../cves/exact-cve-evidence-authority-and-inventory-reads.md) - Specifies how Current CVE authority (the latest consistent reported state, one registered derivation, and its newest completed schema-1 scan) authorizes exact-CVE POA&M verification, how fleet CVE reads choose exact versus legacy authority, and the contract of the /cves, /cve-inventory, and /cve-inventory-page routes.
* [Exact-CVE writer locking and fleet triage transactions](../cves/exact-cve-writer-locking-and-fleet-triage.md) - Defines the READ COMMITTED lock hierarchy for exact-CVE POA&M and deployment-state writers, the SQLSTATE 40001 retry rule, append-only environment dispositions, and the all-or-nothing fleet triage transaction.
