---
type: Historical Reference
title: "UI parity program planning documents"
description: "Catalogs the seven Backlog documents (doc-8 to doc-11 and doc-13 to doc-15) that planned and scored the CrystalForgelatest UI parity program, with status of each."
tags:
  - crystal-forge
  - web-ui
  - design-parity
  - planning
  - backlog
implementation_status: historical
status: deprecated
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: s1
    resource: "Crystal Forge repository file backlog/docs/design/doc-8%20-%20CrystalForgelatest-UI-Parity-Matrix-TASK-328.md at commit 3b23d36f"
    title: "CrystalForgelatest UI Parity Matrix (TASK-328)"
  - id: s2
    resource: "Crystal Forge repository file backlog/docs/design/doc-9%20-%20M16-Baseline-UI-Parity-Scorecard-Initial.md at commit 3b23d36f"
    title: "M16 Baseline UI Parity Scorecard (Initial)"
  - id: s3
    resource: "Crystal Forge repository file backlog/docs/design/doc-10%20-%20CrystalForgelatest-parity-execution-plan.md at commit 3b23d36f"
    title: "CrystalForgelatest parity execution plan"
  - id: s4
    resource: "Crystal Forge repository file backlog/docs/design/doc-11%20-%20CrystalForgelatest-design-source-index.md at commit 3b23d36f"
    title: "CrystalForgelatest design source index"
  - id: s5
    resource: "Crystal Forge repository file backlog/docs/design/doc-13%20-%20Sidebar-surface-execution-map.md at commit 3b23d36f"
    title: "Sidebar surface execution map"
  - id: s6
    resource: "Crystal Forge repository file backlog/docs/design/doc-14%20-%20Parity-execution-playbook-agent-proof.md at commit 3b23d36f"
    title: "Parity execution playbook (agent-proof)"
  - id: s7
    resource: "Crystal Forge repository file backlog/docs/design/doc-15%20-%20Parity-master-plan-and-surface-board.md at commit 3b23d36f"
    title: "Parity master plan and surface board"
---
# UI parity program planning documents

This catalog describes seven Backlog documents that planned and governed the "CrystalForgelatest" UI parity program in mid-2026. The program brought `packages/web-ui` to parity with a Claude-authored design reference. Backlog.md manages the documents by ID, so they stay at their paths under `backlog/docs/design/`. The retained files are the authoritative text. The documents form one family and refer to each other by ID.

> **Status:** historical. The program documents record planning and scoring as of 2026-05-31 to 2026-06-10. The design reference they call "CrystalForgelatest" is a directory outside this repository. The design reference that the repository holds is `docs/design/CrystalForge/`. Task statuses, scores, and the "missing" views below are dated.

## Catalog

| Document | What it is | Status |
| --- | --- | --- |
| [doc-11, CrystalForgelatest design source index](../../../backlog/docs/design/) (file `doc-11 - CrystalForgelatest-design-source-index.md`) | The rule that CrystalForgelatest is the single authoritative design reference. It lists the authoritative files (`app.jsx`, `styles.css`, `components/`) and 23 component references, and says how backlog tasks reference them: name the design file, state the work type, give objective criteria, and never copy JSX or CSS into task text. It ends with a maintenance rule for changes to the design. | Historical. The design source now lives in `docs/design/CrystalForge/`. |
| [doc-8, CrystalForgelatest UI Parity Matrix (TASK-328)](../../../backlog/docs/design/) (file `doc-8 - CrystalForgelatest-UI-Parity-Matrix-TASK-328.md`) | The pass or fail contract for parity work (TASK-328, created 2026-05-31, updated 2026-06-10). It defines global measurements (14-pixel base font, radius scale 6, 10, 14, 16, and 999 pixels, plus the button, input, card border, and focus ring baselines), tolerances (plus or minus 1 pixel spacing, zero tolerance for type size, border, radius, and color token), a global token and primitive contract, a surface matrix for 16 surfaces with design source, route, owner files, visual criteria, mandatory assertions, and required screenshot names (`<view>--<state>--<theme>.png`), an interaction inventory, the screenshot and assertion contracts, a route and owner map, a scoring rubric (visual 40%, interaction 30%, data 20%, verification 10%), and exit criteria. | Historical contract. The weights and grades are reused by the scorecard. |
| [doc-9, M16 Baseline UI Parity Scorecard (Initial)](../../../backlog/docs/design/) (file `doc-9 - M16-Baseline-UI-Parity-Scorecard-Initial.md`) | The initial baseline scorecard for milestone m-16 (created 2026-05-31). It scores ten views from 30 to 78 using the doc-8 weights and ranks eight tasks (TASK-329, 332, 330, 331, 334, 335, 336, 333) as the priority order. It is described as a conservative, planning-grade snapshot to rescore after each parity task lands. | Historical snapshot. Its claim that Compliance and Profile have no view module is stale: `packages/web-ui/src/views/compliance.rs` and `profile.rs` exist and `packages/web-ui/src/routes.rs` routes them. |
| [doc-10, CrystalForgelatest parity execution plan](../../../backlog/docs/design/) (file `doc-10 - CrystalForgelatest-parity-execution-plan.md`) | The delivery strategy "foundation first, then vertical slices". It defines milestones m-18 (Design Parity Foundation), m-19 (Existing Surfaces), m-20 (Missing Surfaces), and m-21 (Final Audit), lists the tasks in each, a prioritized order, and backlog cleanup notes. | Historical plan. |
| [doc-13, Sidebar surface execution map](../../../backlog/docs/design/) (file `doc-13 - Sidebar-surface-execution-map.md`) | A plan that reorganizes UI work by sidebar order. Each of 13 sidebar surfaces has one umbrella task (for example TASK-342 Dashboard, TASK-330 Systems, TASK-338 System Detail, TASK-343 Flakes, TASK-344 Compliance, TASK-349 Caches) with related discrepancy tasks, plus cross-cutting foundations. Child tasks use separate branches and worktrees. | Historical plan. |
| [doc-14, Parity execution playbook (agent-proof)](../../../backlog/docs/design/) (file `doc-14 - Parity-execution-playbook-agent-proof.md`) | An "agent-proof" playbook: how to pick the next task, a standard task procedure (worktree, lock, compare design and view, implement, add a `checks/web-ui` step, verify, open merge request), default verification commands, how to add a check step in `checks/web-ui/tests/integration-test.js`, a route and file map, known gaps, an execution order, a definition of parity, and surface-specific must-dos. Its procedure differs from the current repository agent rules in `AGENTS.md`. | Historical. Use `AGENTS.md` and the agent guide for current procedure. |
| [doc-15, Parity master plan and surface board](../../../backlog/docs/design/) (file `doc-15 - Parity-master-plan-and-surface-board.md`) | The top-level board. It has Phase 0 foundation tasks, a Phase 1 table that maps 14 sidebar surfaces to umbrella and child tasks, Phase 2 closeout tasks, the human loop for promoting tasks, sequencing rules, and a definition of done per surface. | Historical board. |

## Dated claims that no longer match the repository

The retained documents were not edited. A verification pass should confirm these points before anyone relies on the retained text.

- The "known gaps" in doc-14 say that no Compliance view and no Profile view exist, and that `views/cves_old.rs` is dead code to delete. `packages/web-ui/src/views/` now contains `compliance.rs` and `profile.rs`, and no `cves_old.rs` exists.
- doc-14 also lists `systems_mock*.rs`, `flakes_list.rs`, `environments_list.rs`, `policies_api.rs`, and `register_api.rs` for removal. `packages/web-ui/src/views/mod.rs` still declares `systems_mock`, `flakes_list`, and `environments_list`. The status of the removal tasks (for example TASK-297.1) was not checked.
- The design reference path `CrystalForgelatest` is not in the repository. The all-views audit [All-views visual drift audit specification](../ui/all-views-visual-drift-audit-spec.md) records that the design example `docs/design/CrystalForge/` was updated on 2026-07-07 and that this program targeted an older snapshot.
- The design-parity harness that doc-19 prefers exists at `checks/web-ui/design-parity/`.

## Related concepts

- [All-views visual drift audit specification](../ui/all-views-visual-drift-audit-spec.md)
- [Crystal Forge UI/UX Design System](../ui/design-system-overview.md)
- [Task implementation guides](task-implementation-guides.md)
