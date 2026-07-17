# Backlog Workflow for Agents

## Tool selection

Use this order:

1. Read `backlog://workflow/overview` when MCP resources are supported.
2. Otherwise call the Backlog workflow-overview tool.
3. Otherwise enter the repository environment with `nix develop` and use the pinned `backlog` CLI.
4. If none is available, report the blocker rather than editing backlog files by hand without understanding their format.

Search before creating a task to prevent duplicates.

## Task quality

A lightweight task created for a newly discovered issue must contain at least:

- Problem
- Desired outcome
- Status: `Backlog`

A task is sprint-ready only when it contains:

- Clear problem statement
- Goal
- Explicit non-goals
- Objective acceptance criteria
- Architectural constraints
- Verification plan
- Impact areas
- Risk level
- Dependencies

Do not implement a task lacking sprint-ready information. Ask the user to clarify missing decisions that materially affect the implementation.

## Selecting work

Selection order is contextual:

1. Use the task explicitly named by the user.
2. If no identifier was named, use the single existing task that clearly matches the request.
3. If asked to take the next task, choose the highest-priority eligible and unlocked task in `To Do`.
4. If no task matches, create a `Backlog` task. Do not promote it unless a human explicitly selects it.

Never replace the user's requested task with an unrelated higher-priority item.

## Locks

Before implementation, add a note in this form:

```text
LOCK: <agent-name-or-id> on <hostname> in <absolute-worktree-path>
```

If a different active lock exists, do not modify the task or its implementation. Report the lock and ask how to proceed.

Stale locks should be removed only after verifying that the referenced worktree or agent is no longer active, or with explicit maintainer direction.

## Status transitions

### Starting

Requirements:

- Task is `To Do`.
- Task is sprint-ready.
- Dependencies are satisfied.
- No active lock exists.
- Dedicated worktree has been created.

Then move the task to `In Progress` and add the lock before editing application files.

### Review

Move the task to `Review` only after:

- Implementation is complete.
- Required verification passed.
- An MR is open.
- The MR link or identifier is in the task notes.

### Completion

Move the task to `Done` only after:

- The MR is merged into its designated integration branch.
- Required follow-up tasks have been created.
- The task worktree has been removed.

If cleanup fails, record the failure and do not claim the full lifecycle is complete.

## Backlog maintenance mode

For planning, grooming, splitting, deduplication, or task documentation, agents may modify backlog-related files without an implementation worktree.

Allowed scope is limited to the repository's backlog data, templates, and workflow documentation. Before committing, run:

```bash
git status --porcelain
```

Every changed path must be backlog-related. If application, Nix, CI, database, or UI files appear, stop and separate that work into the normal implementation workflow.
