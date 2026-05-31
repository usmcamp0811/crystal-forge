---
id: doc-9
title: M16 Baseline UI Parity Scorecard (Initial)
type: specification
created_date: '2026-05-31 16:13'
tags:
  - m-16
  - parity-scorecard
  - task-328
  - ui-parity
---
# M16 Baseline UI Parity Scorecard (Initial)

Related contract: `doc-8` (CrystalForgelatest UI Parity Matrix, TASK-328)

## Purpose
This is the **initial baseline** scorecard for m-16 parity execution. It is a planning-grade snapshot used to prioritize implementation order and track objective progress to parity.

## Evidence Basis (current snapshot)
- View inventory from `packages/web-ui/src/views/`
- Existing task status and scope in backlog (m-16 and linked parity tasks)
- Presence/absence of dedicated view modules for in-scope surfaces

> Note: This baseline is intentionally conservative. It should be re-scored after each parity task lands with `checks/web-ui` assertions and screenshots.

## Scoring Weights (from doc-8)
- Visual parity: 40%
- Interaction parity: 30%
- Data parity: 20%
- Verification parity: 10%

Grades:
- A: 95-100
- B: 85-94
- C: 70-84
- D: <70

## Baseline Scores (Initial)
| View | Score | Grade | Rationale (initial) |
|---|---:|:---:|---|
| Shell / Layout Frame | 78 | C | Token system exists and shell is functional, but exact parity and strict verification still pending under TASK-329/TASK-333. |
| Systems | 76 | C | Strong existing implementation and active parity work, but strict pixel/data/assertion parity not yet closed (TASK-330). |
| Flakes | 74 | C | Recent rebuild work exists, but legacy cleanup + strict assertion/screenshot contract still pending (TASK-297.1, TASK-331, TASK-333). |
| Builds | 68 | D | Major coherence/density parity work still planned (TASK-275/TASK-331) and verification harness expansion pending. |
| Evaluations | 68 | D | Similar to Builds; structural parity and strict checks still pending (TASK-275/TASK-331/TASK-333). |
| CVEs | 72 | C | View exists with prior improvements, but full m-16 assertion/screenshot parity contract not yet complete. |
| Caches | 77 | C | Recent parity-oriented work exists; still needs strict m-16-wide assertion/screenshot contract completion. |
| Compliance | 35 | D | No dedicated `compliance.rs` view module currently present; net-new view parity task pending (TASK-334). |
| Admin | 62 | D | `admin.rs` exists, but full exact-parity and strict verification scope still pending (TASK-336/TASK-333). |
| User Profile | 30 | D | No dedicated `profile.rs` view module currently present; net-new view task pending (TASK-335). |

## Priority Order (based on impact + dependency path)
1. TASK-329 — Global tokens/shell parity
2. TASK-332 — Backend data-contract parity requirements
3. TASK-330 — Systems parity closure
4. TASK-331 — Cross-view parity closure (Flakes/Builds/Evals/CVEs/Caches/Admin)
5. TASK-334 — Compliance view creation
6. TASK-335 — User Profile view creation
7. TASK-336 — Admin parity completion
8. TASK-333 — Strict verification harness final closure

## Immediate Next Milestone Target
Raise all views to at least **C (>=70)**, then drive remaining surfaces to **B+** with complete screenshot/assertion evidence.

## Re-Scoring Trigger
Re-score after each merged parity task that updates `checks/web-ui` evidence and closes corresponding acceptance criteria.
