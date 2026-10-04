---
type: Design Specification
title: "Evaluation snapshot persistence, bounds, reclamation, and redaction"
description: "Describes content-addressed snapshot persistence, advisory-lock ordering, hard size bounds, the Stage 2 indexed membership cost, orphan reclamation, and the safe-value and redaction policy applied before persistence."
tags:
  - crystal-forge
  - evaluation
  - snapshot
  - persistence
  - redaction
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Evaluation snapshot persistence, bounds, reclamation, and redaction

## Persistence, Bounds, and Reclamation

Option content is redacted and canonicalized before the server computes its
SHA-256 digest. `evaluation_option_contents` stores one payload per digest.
Snapshot rows store only option paths and digest references, so identical
content can be shared across option paths, hosts, and revisions. Flake output
payloads use the same content-addressed pattern at revision scope.

Production option persistence uses parameterized set-oriented batches of at
most 500 rows for content and references. It does not issue two SQL statements
for each option. Digest conflicts update no content and fail the transaction;
the selector advances only after every batch succeeds. Snapshot writers, both
deployment-creation paths, generation retention, and artifact/content reclamation
use one transaction advisory lock. Transactions acquire this advisory lock
before POA&M, system, or deployment row locks. Heartbeat/state ingestion uses
the same order. Therefore deployment creation either
binds an already-published Available artifact or commits first and lets snapshot
finalization bind the exact deployment. If state ingestion commits before
deployment creation, deployment binding reciprocally retains the existing
generation observation. Rollback leaves the old selector and all prior artifacts
unchanged.

The current hard bounds are:

- 256 KiB for one encoded option payload. An over-limit value becomes an
  explicit opaque value and loses oversized provenance.
- 64 MiB for one complete configuration snapshot. An over-limit snapshot
  becomes unavailable.
- 8 MiB for one persisted flake-output payload. An over-limit snapshot becomes
  unavailable.
- 2 MiB for one encoded flake-output API response. An over-limit response
  becomes unavailable rather than returning partial unmarked data.
- 16 KiB of searchable text per option after redaction.
- 16 option-tree levels before a non-empty deeper subtree becomes an explicit
  partial-inventory diagnostic. This guard bounds cyclic or recursively
  generated attribute sets. The option count then covers observed options only.
- 128 retained unreadable-prefix diagnostics. Guarded traversal continues after
  this detail budget is full. The artifact certifies truncation when additional
  prefixes exist; diagnostic overflow does not make the artifact unavailable.

Stage 2 projects the exact healthy Stage-1 paths and builds one option-key
attribute set. Membership lookup is logarithmic in the Nix attribute set rather
than a repeated linear list scan. For `O` observed options and `D` provenance
rows, filtering changes from `O(O * D)` list membership (quadratic when `D`
scales with `O`) to `O(O log O + D log O)`, including index construction.
Complete and partial inventories use the same indexed path.

Foreign keys and immutability triggers prevent mutation or direct removal of
artifact content and references. Server startup and the 15-minute maintenance
loop first releases at most 100 terminal deployment rows that have a snapshot or
derivation binding and whose completion is at least 24 hours old. This interval
lets delayed agent state ingestion retain
the deployed artifact. Active deployment work and retained generation rows are
never released by this step. The loop then removes at most 100 artifacts that
are neither current, retained, nor bound to a deployment and deletes unreferenced
option and flake content in batches of 1,000 rows.
Each pass reports binding, artifact, and content-row progress. The maintenance
loop stops only when all four counts are zero, so more than 100 orphan artifacts
drain in successive bounded transactions. The maintenance transaction uses the
same advisory lock as writers. After a terminal deployment's 24-hour ingestion
window, maintenance releases both its snapshot and exact derivation bindings.
It then removes archived derivations and commits in bounded pages only when no
retained generation, live deployment artifact, system target, or durable request
reservation still references them.

## Safe-Value and Redaction Policy

Redaction runs before persistence, content hashing, search indexing, diffing,
logging of evaluator-controlled diagnostics, or API serialization. The policy
covers option values, nested collection and submodule values, package fields,
module defaults, evaluator errors, source metadata, lock metadata, winner
notes, repository URLs, and dynamic option-inventory diagnostic path
components. Empty or over-limit diagnostic components use one fixed redaction
marker. The server sorts and deduplicates diagnostics after redaction. A
redaction collision marks diagnostic detail as truncated.

The policy replaces all scalar leaves under a sensitive option path. It also
removes nested fields whose normalized names indicate passwords, secrets,
tokens, credentials, private keys, access keys, passphrases, signing keys,
netrc, askpass, API keys, or PATs. Text scanning removes authorization values,
common secret assignments, provider token prefixes, JWT-like values,
high-entropy token-like values, URL user information for any syntactically
valid URL scheme, URL queries and fragments, and SCP-style repository user
information. Redacted names and values are not searchable.

Ordinary strings, booleans, numbers, package metadata, diagnostics, and source
paths remain available when the policy classifies them as safe. Opaque and
failed values remain explicitly typed; the server does not fabricate a value.

Redaction is a deterministic safety boundary, not a general secret-detection
proof. A low-entropy secret under a neutral option path and neutral field name
can evade lexical detection. A safe value can also match a token heuristic and
be redacted. URL credentials, queries, and fragments do not depend on a fixed
scheme allowlist. Operators MUST keep secrets out of evaluator-visible option
metadata and repository URLs. Callers MUST NOT persist an unredacted alternate
copy or log evaluator output before applying this boundary.

## Related concepts

* [Evaluation and Flake Snapshot Architecture](evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot retention, generation rollback, and source reset](evaluation-snapshot-retention-and-rollback.md) - Explains how retained deployment generations keep snapshots, derivations, and commits alive, how generation rollback resolves exact lineage, how branch rewrite and source reset archive commits, and how unavailable content is reported.
* [Evaluation snapshot identity, comparison, and lifecycle](evaluation-snapshot-identity-lifecycle-and-comparison.md) - Specifies full-SHA snapshot identity, first-parent resolution and Changed comparison rules, and the queued/running/failed/available/unavailable lifecycle for Config snapshot reads and the targeted Config inspection mutation.
* [Evaluation snapshot verification expectations](../testing/evaluation-snapshot-verification-expectations.md) - Lists the targeted evidence required for any change to evaluation and flake snapshot architecture, covering PRIMARY isolation, redaction, bounds, identity, non-disclosure, deployment queue behavior, API behavior, and compatibility.
