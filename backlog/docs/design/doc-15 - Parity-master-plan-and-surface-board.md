---
id: doc-15
title: Parity master plan and surface board
type: guide
created_date: '2026-06-10 13:35'
tags:
  - master-plan
  - parity
  - board
  - execution
---
# Parity master plan and surface board

This is the top-level board. Pick ONE umbrella task, hand it to an agent, and it works the child tasks until the surface is Done. Then you review + merge, and move to the next row.

How execution works:
- Each **surface** = one umbrella task = one "row" on the sidebar.
- Each umbrella has **child tasks** (`TASK-N.M`). An agent does them one at a time, each in its own branch/worktree, each merged on its own.
- When all children are merged, the umbrella is Done. You review the surface, then march to the next row.
- Procedure details for every task: `doc-14` (agent-proof playbook).

## Phase 0 — Foundation (do these first; they unblock all surfaces)
| Order | Task | What it does |
|---|---|---|
| 0.1 | `TASK-329` | tokens + sidebar groups + topbar notifications parity |
| 0.2 | `TASK-341` | remove dead/legacy files + fix duplicate backlog metadata |
| 0.3 | `TASK-328` | keep parity matrix `doc-8` current (reference, ongoing) |

## Phase 1 — Sidebar surfaces (march top to bottom)
| # | Surface | Umbrella | Child tasks | Notes |
|---|---|---|---|---|
| 1 | Dashboard | `TASK-342` | `342.1` mock removal, `342.2` grid+visuals | builds on `TASK-321` |
| 2 | Systems | `TASK-330` | `330.1` mock removal, `330.2` layout, `330.3` panel+modals | |
| 3 | System Detail | `TASK-338` | `338.1` tabs+overview, `338.2` deploy/cves/config | also `268` history, `277` logs, `295` icons |
| 4 | Environments | `TASK-339` | `339.1` view+CRUD | |
| 5 | Flakes | `TASK-343` | `343.1` list/timeline, `343.2` modals/sync | legacy cleanup `297.1` |
| 6 | Evaluations | `TASK-345` | `345.1` queue+drawer | coherence `275` |
| 7 | Builds | `TASK-347` | `347.1` queue+detail+real actions | coherence `275` |
| 8 | CVEs / Scanning | `TASK-348` | `348.1` cves+delete legacy, `348.2` scanning polish | worker `327` |
| 9 | Policies | `TASK-340` | `340.1` view+editor modal | |
| 10 | Builders | `TASK-346` | `346.1` view+modal | bug `204`, runtime `291` |
| 11 | Caches | `TASK-349` | `349.1` mock removal+parity | view largely done (`303`) |
| 12 | Admin | `TASK-336` | `336.1` view+real data | |
| 13 | Compliance | `TASK-344` | backend `312..317` → UX `319` → parity `334` | see `doc-12` |
| 14 | Profile | `TASK-335` | net-new route+view | |

## Phase 2 — Closeout
| Order | Task | What it does |
|---|---|---|
| 2.1 | `TASK-333` | strict screenshot/assertion harness closure |
| 2.2 | rescore `doc-9` | confirm every surface reaches B+ parity |

## The loop you run as the human
1. Confirm Phase 0 is merged (foundation).
2. Pick the next surface row's **umbrella** task and promote its **child tasks** from Backlog → To Do.
3. Tell an agent: "Work `TASK-330` to completion via doc-14, doing each child task as its own MR."
4. Review each child MR; merge.
5. When the umbrella's children are all merged, mark the umbrella Done.
6. Go to the next row.

## Sequencing rules (so agents don't trip each other)
- Within a surface, do `*.1` (mock/data truth) before `*.2`/`*.3` (layout/modals) when both exist.
- Compliance (row 13) is gated by its backend roadmap (`doc-12`); don't start the UI parity (`334`) until `317` + `319` land.
- Foundation (`329`) should land before deep per-view CSS work to avoid rework.

## Definition of Done for a surface (review checklist)
- Layout/typography/spacing match design (doc-8 tolerances).
- All primary values come from the real API (no mock/fallback in production path).
- Loading/empty/error/populated states styled per design.
- Each design control works (filters/search/toggle/tabs/modals).
- `checks/web-ui` has a step that screenshots + asserts a real interaction.
- `cargo fmt`, web-ui `cargo check` (wasm), and `nix build .#checks.x86_64-linux.web-ui` pass.

## Reference docs
- `doc-14` agent-proof playbook (how to do any task)
- `doc-8` parity matrix (measurable per-view criteria)
- `doc-9` baseline scorecard (rescore after merges)
- `doc-12` compliance roadmap
- `doc-13` sidebar surface execution map
