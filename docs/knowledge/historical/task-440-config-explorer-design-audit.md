---
type: Historical Reference
title: "TASK-440 Config Explorer design audit"
description: "Records the TASK-440 design audit of the Config side column and Flake Modules pane against the reference design, the intentional differences, geometry-sensitive Config pagination, and the screenshot evidence policy."
tags:
  - crystal-forge
  - config-explorer
  - design-audit
  - task-440
  - ui
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# TASK-440 Config Explorer design audit

> **Status:** task-bound audit record. The behavior it describes exists in the current Web UI (for example the 10 to 80 row geometry clamp in `packages/web-ui/src/views/system_detail.rs`). The audit itself is a point-in-time record of TASK-440.

## TASK-440 Design Audit

The Config side column preserves the reference order: Modules, Evaluation, and
Drift. Modules incrementally loads bounded source rows with input, path, and
winning/defined counts while displaying the exact snapshot-wide total.
Evaluation contains selected-revision completion, duration, option total,
source total, toplevel store path, closure package count, and comparison
identity when those persisted facts exist. Drift compares the exact selected
and running store paths. Each card has separate loading, error, lifecycle,
empty, and unavailable states. The layout uses the reference 7:5 wide split and
one-column narrow layout.

The Flake Modules pane orders modules by authoritative consumer count, shows a
proportional blast-radius indicator, and renders declarations in an explicit
Option/Type/Default table. Declaration continuation, replacement, loading,
retry, error, and unavailable states remain local to the expanded module.

The implementation intentionally differs from the reference in these places:

- The Inputs pane retains additional authoritative lock-resolution metadata.
  The column hierarchy therefore is not an exact copy of the reference.
- Browser fixtures use deterministic values instead of the reference's random
  fixture values. The changed reference `.thumbnail` is also excluded. These
  fixture differences are not product behavior.

Config pagination is geometry-sensitive. The browser measures the natural
height of the three side cards and their rendered gaps. It subtracts table
chrome and header height, divides by the rendered row height, and clamps the
request limit to 10-80 rows. Invalid measurements retain the previous limit;
the initial fallback is 24. A material limit change resets the offset to zero.
The server still enforces its 1-100 response bound.

Deterministic screenshots in dark and light themes at 1920x1080 and 900x900,
together with semantic and geometry assertions, are the visual evidence. The
assertions cover ratios, columns, stacking, clipping, overlap, reachability, and
inner scrolling. Screenshot baseline and rendered-design comparisons remain
advisory and non-blocking. They are not a strict automated pixel-baseline gate.

## Related concepts

* [Evaluation snapshot API and URL state](../api/evaluation-snapshot-api-and-url-state.md) - Describes server-side option search, filter, pagination and snapshot tokens (409 snapshot_changed), flake output paging, module declaration and module-source endpoints, summary field meanings, tracked provenance, and System Detail URL state.
* [Config Explorer Architecture](../evaluation/config-explorer-architecture.md) - Specifies the Config Explorer design: why full option crawls are the wrong prerequisite, the three-evaluator authority invariants, the phased lazy inspection model, the Configured options classifier, and the benchmark record.
* [Config Explorer current implementation map](../evaluation/config-explorer-implementation-status.md) - Maps the Config Explorer design to its implementing server, worker, Nix expression, query, migration, API, and Web UI paths, and describes how scoped observations, V2 snapshot reuse, and paged root and prefix observations currently work.
