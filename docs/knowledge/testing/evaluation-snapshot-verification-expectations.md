---
type: Testing Guide
title: "Evaluation snapshot verification expectations"
description: "Lists the targeted evidence required for any change to evaluation and flake snapshot architecture, covering PRIMARY isolation, redaction, bounds, identity, non-disclosure, deployment queue behavior, API behavior, and compatibility."
tags:
  - crystal-forge
  - evaluation
  - testing
  - verification
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Evaluation snapshot verification expectations

## Verification Expectations

Changes to this architecture require targeted evidence for:

- PRIMARY isolation from option trees, module graphs, exported modules, and
  original derivation metadata, while policy data remains available;
- unavailable lifecycle recording when no separate exploration artifact exists;
- pre-persistence redaction across values, defaults, errors, paths, URLs,
  storage, search, diffs, logs, and API-shaped reads;
- content deduplication, all size bounds, corrupt-content degradation, and
  bounded orphan reclamation;
- full-SHA prefix collisions, Git root/missing-parent behavior, generation
  baselines, branch rewrite, source reset, retained generations, and store GC
  independence;
- environment non-disclosure and authoritative reconciliation/counts across
  endpoints;
- explicit queue authorization and reuse, concurrent deployment reservations,
  conflicting intents, conversion failure, partial success, retry replay, and
  legacy compatibility;
- API pagination, search/filter counts, response bounds, stale browser response
  rejection, hard reload, and browser back/forward URL restoration;
- supported agent and builder compatibility without a protocol migration; and
- applicable Rust tests, rustdoc, formatting, SQLx/migration checks, web UI
  build, authoritative browser workflows, and broader Nix checks when affected.

## Related concepts

* [Evaluation and Flake Snapshot Architecture](../evaluation/evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot persistence, bounds, reclamation, and redaction](../evaluation/evaluation-snapshot-persistence-bounds-and-redaction.md) - Describes content-addressed snapshot persistence, advisory-lock ordering, hard size bounds, the Stage 2 indexed membership cost, orphan reclamation, and the safe-value and redaction policy applied before persistence.
* [Evaluation snapshot API and URL state](../api/evaluation-snapshot-api-and-url-state.md) - Describes server-side option search, filter, pagination and snapshot tokens (409 snapshot_changed), flake output paging, module declaration and module-source endpoints, summary field meanings, tracked provenance, and System Detail URL state.
