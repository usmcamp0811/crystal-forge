# AI Sprint Planning & Backlog Grooming Guide

This document defines how sprint planning and backlog grooming are conducted for AI-executed development cycles.

This is separate from `AGENTS.md`.

The purpose of this process is to:

- Clarify scope before execution
- Eliminate ambiguity
- Reduce rework
- Maintain architectural integrity
- Enable compressed, high-velocity AI sprints

---

# Sprint Model

## Sprint Length

**2–3 day equivalent (AI-compressed)**

AI execution is fast. Planning must be tighter.

### AI Compression Rules

- Tasks must be small enough to complete in a single execution cycle.
- No multi-day epics.
- No partially implemented abstractions.
- No speculative architecture.
- Every task must produce shippable value.
- No mid-sprint scope expansion.

---

# Roles

**Human**

- Product Owner
- Architecture Guardrail
- Sprint Reviewer

**AI Agents**

- Strict task executors
- Operate only within defined acceptance criteria

---

# Sprint Lifecycle

## Phase 1 — Plan (30–60 minutes)

- Groom backlog
- Clarify ambiguous tasks
- Split oversized items
- Identify dependencies
- Define sprint goal
- Lock sprint scope

## Phase 2 — Execute

- AI agents execute only selected tasks
- No scope changes during sprint
- No implicit refactors
- No untracked work

## Phase 3 — Review

- Verify builds
- Verify tests
- Review architecture boundaries
- Check performance regressions
- Confirm acceptance criteria

## Phase 4 — Retro Planning

- Identify ambiguity that caused friction
- Identify rework patterns
- Tighten acceptance criteria style
- Improve task slicing for next sprint

---

# Backlog Grooming Prompt

Use the following prompt when planning a sprint.

---

## SPRINT PLANNING MODE

```
You are operating as a Senior Technical Program Manager and Staff Engineer.

Your role is to help conduct backlog grooming and sprint planning for this repository.

We are planning a compressed AI-executed sprint (2–3 day equivalent).

Objectives:
- Clarify scope
- Eliminate ambiguity
- Slice work into well-defined tasks
- Identify dependencies
- Minimize rework
- Protect architectural integrity

Do NOT write code.
Do NOT implement.
This is planning only.

----------------------------------------------------

PROJECT CONTEXT:
[Insert repository summary]

CURRENT ARCHITECTURE:
[Insert architecture summary: Rust backend, Dioxus frontend, sqlx, Nix, etc.]

CURRENT BACKLOG:
[Insert backlog or summary]

CONSTRAINTS:
- Must use Nix dev environment
- Must maintain clean architecture
- No scope creep inside sprint
- AI agents execute strictly by acceptance criteria

----------------------------------------------------

PLANNING GOALS:

1. Groom backlog:
   - Remove vague tasks
   - Split oversized tasks
   - Add missing acceptance criteria
   - Identify technical debt items

2. Define Sprint Scope:
   - Choose a focused theme
   - Limit WIP
   - Ensure tasks are independently completable
   - Avoid cross-cutting partial work

3. Risk Review:
   - Identify high-risk tasks
   - Identify tasks needing spikes
   - Flag unclear architectural areas

4. Produce Output:
   - Sprint Goal (1–2 sentences)
   - Selected Tasks
   - Task Order
   - Dependencies Map
   - Explicit "Out of Scope"
   - Definition of Done for this sprint

Ask clarifying questions before proposing the sprint plan.
```

---

# Task Format Standard

All sprint tasks must be defined using the following structure:

```
Title:
Problem:

Acceptance Criteria:
- ...
- ...

Non-Goals:
- ...

Files Likely Touched:
- ...

Verification Commands:
- nix develop -c cargo check
- nix develop -c cargo test
- nix develop -c cargo fmt -- --check
- nix develop -c cargo clippy -- -D warnings

Risk Level:
Dependencies:
```

If acceptance criteria are incomplete, implementation will be incorrect.

---

# AI-Safe Task Design Principles

## 1. Vertical Slices Over Layered Work

Avoid:

- “Add domain models”
- “Implement DB layer”
- “Create UI component”

Prefer:

- “User can create X via UI; validated; persisted; tested”

Every task should result in observable, testable behavior.

---

## 2. Remove Ambiguous Language

Avoid:

- Improve
- Refactor
- Clean up
- Optimize
- Enhance

Replace with:

- Reduce function length below 75 lines
- Remove unwrap in module X
- Replace global state with injected trait
- Add explicit error enum

---

## 3. Minimize Coupling

Tasks should:

- Touch minimal modules
- Avoid cross-cutting concerns
- Be independently reviewable
- Not depend on incomplete work

---

## 4. Explicit Non-Goals

Each task must state what it will NOT do.

This prevents accidental scope creep during AI execution.

---

# Sprint Definition of Done

A sprint is complete when:

- All selected tasks are marked Done
- Builds succeed in Nix environment
- Tests pass
- Lint and formatting pass
- Acceptance criteria are verified
- No partial abstractions remain
- No untracked architectural changes occurred

---

# Risk Control Guidelines

Flag tasks as high risk if they involve:

- Schema changes
- Cross-layer refactors
- Authentication/authorization logic
- State management changes
- Dependency updates
- Architectural boundary adjustments

High-risk tasks should be:

- Isolated
- Small
- Explicitly verified

---

# Explicit Out-of-Scope Section

Each sprint must include:

```
Out of Scope:
- ...
- ...
```

If work is discovered mid-sprint:

- It goes to backlog
- It does not enter active sprint

---

# Recommended Documentation Pattern

Create a `SPRINT.md` file per sprint:

```
# Sprint X

## Sprint Goal
...

## Selected Tasks
- TASK-1
- TASK-2
- TASK-3

## Out of Scope
- ...

## Definition of Done
...
```

Agents read `SPRINT.md` before execution begins.

---

# Guiding Principle

Assume the execution agent will do exactly and only what is written.

If acceptance criteria are incomplete, the implementation will be wrong.

Over-specify outcomes.
Under-specify nothing.

---

# Summary

This sprint system is designed for:

- High-velocity AI execution
- Strict architectural control
- Reduced ambiguity
- Deterministic builds
- Clean iteration loops

Plan tightly.
Execute strictly.
Review aggressively.
Iterate intelligently.
