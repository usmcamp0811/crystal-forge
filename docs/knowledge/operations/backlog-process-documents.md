---
type: Operator Guide
title: Backlog process documents
description: Pointer and status record for the Backlog.md Task Template document (doc-1), listing the sections every well-formed task description contains and where the retained file lives.
tags:
  - crystal-forge
  - backlog
  - process
  - task-template
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:58:30-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file backlog/docs/doc-1%20-%20Task-Template.md at commit 3b23d36f"
    title: Task Template
---

# Backlog process documents

This concept is a navigation and status record. Backlog.md manages the
documents under `backlog/docs/` by document ID, and tasks reference their
paths, so the files stay in place. The authoritative text of the Task Template
is the retained file `backlog/docs/doc-1 - Task-Template.md`, reached from this
file as `../../../backlog/docs/doc-1 - Task-Template.md`. The file name contains
spaces, so this concept gives the path as code and not as a Markdown link. The
`%20` in the `sources` entry encodes those spaces because the provenance
descriptor does not allow whitespace in a path.

## Task Template (`doc-1`)

The Task Template is a Backlog.md document of type `other`, created
2026-02-19, that defines the sections of a task description. Each section
carries an HTML comment that says what to write. The template ends every
section with a horizontal rule and starts with the status `Backlog`.

| Section | Purpose stated in the template |
| --- | --- |
| Title | A short, specific, outcome-focused title. Avoid vague titles such as "Refactor stuff". |
| Status | The lifecycle state. The template starts at `Backlog`. |
| Problem Statement | What is wrong, missing, unclear, or inefficient, and why it matters, in plain language. |
| Goal | What must be true after the task, described as an outcome and not as an implementation. |
| Non-Goals | What the task does not include, to prevent scope creep (for example no styling, API, or database changes). |
| Acceptance Criteria | Objective and testable criteria. If the list is unclear, the task is not ready for `To Do`. |
| Architectural Constraints | Required boundaries, for example no business logic in the UI and no new global state. |
| Implementation Notes | Optional hints. They are not required for execution and avoid prescribing exact code. |
| Verification Plan | How completion is verified. The template lists automated checks (`nix flake check`, `cargo test`, `cargo clippy -- -D warnings`) and manual steps. |
| Impact Analysis | The affected areas: UI, API, domain, infrastructure, database. |
| Risk Level | `Low`, `Medium`, or `High`, with the reason. |
| Dependencies | Blocking tasks. Empty when there are none. |
| Follow-Up Work | Improvements found but excluded. They must become separate Backlog tasks. |

Key decisions recorded in the template: acceptance criteria gate the move from
`Backlog` to `To Do`, non-goals are explicit, and discovered extra work becomes
separate Backlog tasks instead of expanding the task.

## Implementation status and evidence

Status: implemented as a document. The template is a text convention. No code
validates that a task contains these sections. The migration found no
reference to `doc-1` or the template in `AGENTS.md`, `CLAUDE.md`, or
`docs/agents/` at the migration base commit. The lifecycle names the template
uses (`Backlog`, `To Do`) match the lifecycle in `AGENTS.md`. Backlog.md task
creation uses its own structured fields (acceptance criteria, definition of
done, and so on), so the template is guidance and not the task file format.

## Related concepts

- [AI sprint planning and backlog grooming guide](ai-sprint-planning.md), which
  defines a different task format with `Title`, `Problem`, `Acceptance
  Criteria`, `Non-Goals`, `Files Likely Touched`, `Verification Commands`,
  `Risk Level`, and `Dependencies`.
- [Contributing guide](contributing-guide.md)
