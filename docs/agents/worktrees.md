# Worktree Workflow

Integration worktrees are for integration and inspection. Do not implement, commit, rebase, or merge from the `main` or `dev` worktree.

Do not assume a fixed clone location. Discover existing worktrees with:

```bash
git worktree list --porcelain
```

Identify integration worktrees by their checked-out branches, then explicitly verify both are clean:

```bash
git -C /absolute/path/to/main-worktree status --porcelain
git -C /absolute/path/to/dev-worktree status --porcelain
```

If either integration worktree is dirty, report its paths. Do not discard or move those changes without the user's direction.

## Create a task worktree

Use a branch containing the task identifier:

```text
TASK-ID-short-slug
```

From an existing repository worktree, update knowledge of the designated base without modifying user work, then create the worktree. Normally the base is `dev`:

```bash
git worktree add -b TASK-ID-short-slug /absolute/path/to/TASK-ID-short-slug dev
```

If the branch already exists:

```bash
git worktree add /absolute/path/to/TASK-ID-short-slug TASK-ID-short-slug
```

Do not reuse a worktree belonging to another task.

## Verify before editing

Run these inside the new worktree:

```bash
pwd
git rev-parse --abbrev-ref HEAD
git rev-parse --show-toplevel
git status --porcelain
git worktree list
```

Confirm:

- The working directory and repository root are the intended dedicated worktree.
- The branch contains the task identifier.
- The worktree is clean, unless the user explicitly supplied existing changes for this task.
- The base branch matches the active task.

Report a concise preflight summary. Retain raw output when needed for troubleshooting, but do not flood the user with unrelated worktree metadata by default.

## Existing changes

If the task worktree already contains changes:

- Inspect them before writing.
- Treat them as user or other-agent work unless proven otherwise.
- Continue only if they are clearly part of the same task and do not conflict.
- Never use `git reset --hard`, `git checkout --`, or broad cleanup commands to make the worktree clean.

## Cleanup

After the MR is merged or the user explicitly abandons the task, run from another worktree:

```bash
git worktree remove /absolute/path/to/TASK-ID-short-slug
git worktree prune
```

Do not force removal of a dirty worktree without explicit user approval and a clear accounting of the uncommitted changes.
