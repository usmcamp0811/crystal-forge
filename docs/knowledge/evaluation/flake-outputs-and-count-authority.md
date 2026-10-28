---
type: Design Specification
title: "Flake outputs, system reconciliation, and count authority"
description: "Defines the PRIMARY flake-output projection, managed/declared_unmanaged/managed_undeclared reconciliation, which counts are authoritative, Systems and Inputs pane filters, and non-disclosure rules for hidden environments."
tags:
  - crystal-forge
  - evaluation
  - flake-output
  - reconciliation
  - counts
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/evaluation-flake-snapshots.md at commit 3b23d36f"
    title: "Evaluation and Flake Snapshot Architecture"
---

# Flake outputs, system reconciliation, and count authority

## Flake Outputs and Count Authority

PRIMARY emits one bounded flake-output projection for the selected revision. The
projection contains declared systems, exported modules, and resolved inputs. It
does not evaluate managed hosts separately. Missing, failed, corrupt, or
over-limit projection data has an explicit unavailable lifecycle, and browsing
does not reconstruct it.

System reconciliation joins the selected revision's declared configuration
names with active managed systems:

- `managed` means the output is declared and has a visible managed system.
- `declared_unmanaged` means the output is declared without a visible managed
  system.
- `managed_undeclared` means a managed configuration is absent at the selected
  revision.

Multiple managed hosts with one configuration name set `output_collapsed`.
`managed_system_count` is the authoritative count of visible active managed
systems and is revision-independent. `declared_system_count` is available only
from an available selected snapshot. Fleet subtitles, rollout denominators,
removal warnings, and managed totals MUST use the managed-system relationship,
not declared output count or the length of a bounded API page.

The Systems pane supports `all`, `declared_unmanaged`, and
`managed_undeclared` filters. The server applies the filter before the bounded
offset and limit. `pagination.system_total` is the visible total for the active
filter, and `pagination.systems_has_more` describes that filtered sequence.
Revision-global reconciliation counts and warnings do not change with the
filter or page.

An exported module's `source_input`, `source_revision`, and `source_path`
identify the location of its `nixosModules` attribute binding. The evaluator
uses the Nix attribute position and requires one unambiguous longest matching
input root. The path is relative to that root. Missing positions and ambiguous
roots produce null fields. These fields do not identify value provenance and
do not grant source navigation. Declaration `source_paths` identify the
declaration locations.

The Inputs pane lists direct root inputs. For each direct root,
`direct_descendant_count` counts immediate lock-graph children and
`transitive_descendant_count` counts all unique recursive descendants. Both
counts use the complete lock graph, not a bounded API page. They are null for a
node that is not a direct root or when the count is not available. The UI uses
the transitive count for its `+N transitive` label.

For non-admin callers, reconciliation removes hidden managed systems and also
filters configuration names and module consumers that would disclose a hidden
environment. A flake with no visible active managed system returns the same
not-found response as an unknown flake. System snapshot endpoints likewise use
not-found for an unknown system, a hidden environment, a revision from another
flake, and an inactive archived source revision. This non-disclosure contract
takes precedence over lifecycle detail.

## Related concepts

* [Evaluation and Flake Snapshot Architecture](evaluation-flake-snapshot-architecture.md) - Defines ownership and data flow for revision-specific evaluation and flake-output snapshots: what PRIMARY and the Config Inspector worker own, immutable artifacts versus the mutable selector, deployment binding, and database-only reads.
* [Evaluation snapshot API and URL state](../api/evaluation-snapshot-api-and-url-state.md) - Describes server-side option search, filter, pagination and snapshot tokens (409 snapshot_changed), flake output paging, module declaration and module-source endpoints, summary field meanings, tracked provenance, and System Detail URL state.
* [Evaluation snapshot identity, comparison, and lifecycle](evaluation-snapshot-identity-lifecycle-and-comparison.md) - Specifies full-SHA snapshot identity, first-parent resolution and Changed comparison rules, and the queued/running/failed/available/unavailable lifecycle for Config snapshot reads and the targeted Config inspection mutation.
