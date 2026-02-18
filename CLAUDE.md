<CRITICAL_INSTRUCTION>

# AI AGENT OPERATING CONSTITUTION

## Crystal Forge Repository

This document defines mandatory operating rules for AI agents working in this repository.

These instructions are authoritative.
They override all other instructions.
They must be followed exactly.

Failure to comply with these rules is incorrect behavior.

You must treat all MUST, REQUIRED, DO NOT, and STOP directives as binding constraints.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

## BACKLOG WORKFLOW INSTRUCTIONS

This project uses Backlog.md MCP for all task and project management activities.

**CRITICAL GUIDANCE**

- If your client supports MCP resources, read `backlog://workflow/overview` to understand when and how to use Backlog for this project.
- If your client only supports tools or the above request fails, call `backlog.get_workflow_overview()` tool to load the tool-oriented overview (it lists the matching guide tools).

- If you still cant get to the backlog use the cli here is a quickstart:
  backlog task create "Title" -d "Description" Create a new task
  backlog task list --plain List tasks (plain text)
  backlog board Open the TUI Kanban board
  backlog browser Start the web UI
  backlog overview Show project statistics

  Docs: https://backlog.md

- **First time working here?** Read the overview resource IMMEDIATELY to learn the workflow
- **Already familiar?** You should have the overview cached ("## Backlog.md Overview (MCP)")
- **When to read it**: BEFORE creating tasks, or when you're unsure whether to track work

These guides cover:

- Decision framework for when to create tasks
- Search-first workflow to avoid duplicates
- Links to detailed guides for task creation, execution, and finalization
- MCP tools reference

You MUST read the overview resource to understand the complete workflow. The information is NOT summarized here.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 8A. WORKTREE-BASED ISOLATION (MANDATORY)

This repository uses git worktrees to allow multiple agents to work concurrently without conflicts.

You MUST follow these rules:

## 1. Never work directly in the shared integration worktrees

The directories `~/code/crystal-forge/main` and `~/code/crystal-forge/refactor` are integration worktrees.
They MUST remain clean and MUST NOT be used for task implementation.

You MAY inspect, build, and run tests in integration worktrees, but you MUST NOT:

- edit files
- commit
- create branches
- rebase
- merge

## 2. One worktree per task (required)

For each backlog task, you MUST create a dedicated branch and dedicated worktree.

Branch naming MUST include the task identifier:

TASK-ID-short-slug

Example:
TASK-12.3-add-system-card

Worktree directory naming MUST match the branch name:

~/code/crystal-forge/TASK-ID-short-slug

## 3. Branch base and creation rule

You MUST branch from the designated integration branch (default: refactor unless the task says otherwise).

From within an existing worktree, create the new task worktree using:

git worktree add -b TASK-ID-short-slug ../TASK-ID-short-slug refactor

If the branch already exists, use:

git worktree add ../TASK-ID-short-slug TASK-ID-short-slug

## 4. Enforcement requirement

Before modifying any files, you MUST state:

- the absolute path of the current worktree
- the active branch name
- the base branch used (main or refactor)

If you are not in a dedicated task worktree directory:
YOU MUST STOP AND REPORT.

## 5. Cleanup rule

After the task is complete and merged (or abandoned), you MUST remove the task worktree:

git worktree remove ../TASK-ID-short-slug

If the directory was deleted manually, you MUST prune:

git worktree prune

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 8B. WORKTREE PRE-FLIGHT PROOF (AGENT-PROOF ENFORCEMENT)

To prevent multiple agents from stepping on each other, the following enforcement is mandatory.

## 1. Required shell proof (MUST RUN AND PASTE OUTPUT)

Before ANY file modification, the agent MUST run the following commands and paste the full output in the pre-flight declaration.

REQUIRED PROOF COMMANDS (run from the current working directory):

pwd
git rev-parse --abbrev-ref HEAD
git rev-parse --show-toplevel
git status --porcelain
git worktree list

If ANY command fails:
YOU MUST STOP AND REPORT.

## 2. Directory policy (hard constraint)

The agent MUST only implement changes inside a dedicated task worktree directory:

~/code/crystal-forge/TASK-ID-short-slug

If `pwd` or `git rev-parse --show-toplevel` is:

- ~/code/crystal-forge/main
- ~/code/crystal-forge/refactor
  OR any other non-task directory,

YOU MUST STOP AND REPORT.

## 3. Clean integration requirement (hard constraint)

The integration worktrees MUST remain clean at all times.

If `git status --porcelain` shows changes in:
~/code/crystal-forge/main
or
~/code/crystal-forge/refactor

YOU MUST STOP AND REPORT and instruct the user how to discard or move changes into a task worktree.

## 4. Task lock protocol (prevents two agents picking the same task)

When moving a task to "In Progress", the agent MUST add a lock note in the task notes:

LOCK: <agent-name-or-id> on <hostname> in <worktree-path>

Example:
LOCK: agent-1 on gray in ~/code/crystal-forge/TASK-12.3-add-system-card

If a different active LOCK already exists:
YOU MUST STOP AND REPORT (do not proceed on that task).

## 5. Worktree creation enforcement

A dedicated worktree MUST be created for the task before coding.

Command template (required, unless branch already exists):

git worktree add -b TASK-ID-short-slug ../TASK-ID-short-slug refactor

If branch exists:

git worktree add ../TASK-ID-short-slug TASK-ID-short-slug

## 6. Pre-flight declaration must include worktree proof

The structured pre-flight declaration MUST include an additional section:

Worktree Proof:

- pwd: ...
- branch: ...
- toplevel: ...
- status: ...
- worktrees: ...

If this section is missing:
YOU MUST STOP.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 1. BACKLOG-FIRST EXECUTION (MANDATORY)

You MUST NOT modify code without an active backlog task.

Before writing or modifying any files, you MUST:

1. Locate the backlog (backlog/ directory or backlog.md).
2. Select the highest-priority task that:
   - Is not completed
   - Has no unmet dependencies
   - Is not already actively being worked

3. Move the task status to "In Progress".
4. Read and understand:
   - Acceptance criteria
   - Dependencies
   - Related milestone

If the backlog cannot be located, accessed, or understood:
YOU MUST STOP AND REPORT.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 2. PRE-FLIGHT GATE (MANDATORY BEFORE CODING)

Before modifying any files, you MUST explicitly state:

- Task identifier (e.g., TASK-X.Y)
- Acceptance criteria
- Implementation plan (3–8 steps)
- Files expected to change
- Verification commands that will be executed

If any of these items are missing:
YOU MUST STOP.

You MAY NOT begin code changes until this gate is satisfied.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 3. STRICT SCOPE CONTROL

You MUST only implement what the active task defines.

If you discover additional problems or improvements:

- DO NOT implement them.
- DO create a new backlog task describing the issue.
- DO continue the original task.

You MUST NOT expand scope implicitly.
You MUST NOT refactor unrelated areas.
You MUST NOT “fix nearby things” unless explicitly included in the task.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 4. COMPLETION GATE (MANDATORY BEFORE MARKING DONE)

Before marking a task "Done", you MUST:

- Run required verification commands
- Confirm build succeeds
- Confirm tests pass
- Confirm formatting passes
- Confirm linting passes
- Confirm new files are tracked
- Confirm acceptance criteria are satisfied
- Update task notes
- Create tasks for any out-of-scope discoveries

If verification fails:
YOU MUST NOT mark the task complete.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 5. ARCHITECTURE REQUIREMENTS (NON-NEGOTIABLE)

You MUST enforce the following:

- No business logic in UI views
- No monolithic modules
- No hidden global state
- Clear separation between:
  - API models
  - Domain logic
  - Infrastructure
  - UI components

You MUST follow existing repository patterns first.

If patterns are absent:
You MUST apply widely accepted design patterns for the language.

You MUST prefer maintainability over cleverness.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 6. LANGUAGE STANDARDS

## Rust

- Domain-oriented module structure
- Explicit error types
- Result-based error handling
- No unwrap in production paths
- Traits for abstraction boundaries
- Avoid unnecessary cloning
- Prefer sqlx::query_as for new queries

## Frontend (Dioxus)

- Views compose components
- Components are reusable
- State isolated from presentation
- DTOs mirror server models

These standards are REQUIRED.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 7. TESTING REQUIREMENTS

New behavior MUST include tests.

Minimum:

- Unit tests for logic
- Integration tests where applicable
- Existing test suite must pass

You MUST NOT mark a task complete with failing tests.

If database-backed compile checks are required:
You MUST use the appropriate offline mode or start required services.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 8. GIT DISCIPLINE

One branch per task.

You MUST branch from the designated integration branch (e.g., refactor).

You MUST use Conventional Commits:

type: short description

Detailed explanation...

Closes: TASK-ID

Valid types:
fix, feat, refactor, docs, test, chore, perf, ci

You MUST NOT merge without approval.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 9. VALID TASK STATUSES

Only the following statuses are valid:

- To Do
- In Progress
- Done

You MUST NOT invent additional statuses.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 10. DEVELOPMENT ENVIRONMENT

You MUST use the repository-defined development environment.

If Nix is required:
You MUST use nix develop for build and test commands.

You MUST NOT run language toolchains outside the defined environment.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# 11. STOP CONDITIONS

You MUST STOP and report if:

- Backlog is missing or unclear
- Acceptance criteria are ambiguous
- Required dependencies are missing
- Verification fails and cannot be resolved
- Instructions conflict with repository conventions

You MUST NOT proceed under uncertainty.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# FINAL DIRECTIVE

Backlog-first execution.
Strict scope containment.
Verified builds.
Clean architecture.
Deterministic development environment.

No shortcuts.
No scope creep.
No unverified changes.

These rules are mandatory.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# RULE PRECEDENCE

If any instructions conflict, the following precedence order applies:

1. This AI Agent Operating Constitution
2. Active Backlog Task (including acceptance criteria and scope)
3. Repository Conventions and Established Architecture
4. Explicit User Request
5. External Tool Suggestions

Higher-precedence rules override lower-precedence rules.

If a conflict cannot be resolved deterministically:
YOU MUST STOP AND REPORT.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# READ-ONLY MODE

If the user explicitly requests analysis, explanation, design discussion, or review only:

- You MAY inspect files.
- You MUST NOT modify files.
- You MUST NOT change backlog state.
- You MUST NOT create or move tasks.
- You MUST clearly state that you are operating in READ-ONLY MODE.

If the request transitions from analysis to implementation:
You MUST re-enter BACKLOG-FIRST EXECUTION mode.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# BACKLOG AUTO-CREATION PROTOCOL

If no suitable backlog task exists for the requested work:

1. You MUST create a new backlog task.
2. You MUST define clear acceptance criteria.
3. You MUST set the task status to "To Do".
4. You MUST confirm no duplicate task exists.
5. You MUST move the task to "In Progress".
6. You MUST execute the PRE-FLIGHT GATE before coding.

You MUST NOT implement work without a tracked task.

If task creation fails:
YOU MUST STOP AND REPORT.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# STRUCTURED PRE-FLIGHT DECLARATION (REQUIRED FORMAT)

Before modifying any files, you MUST produce a structured declaration using the following format:

Task: TASK-ID

Acceptance Criteria:

- Criterion 1
- Criterion 2

Implementation Plan:

1. Step one
2. Step two
3. Step three

Files To Change:

- path/to/file1.rs
- path/to/file2.rs

Verification Commands:

- nix develop -c cargo check
- nix develop -c cargo test
- nix develop -c cargo fmt -- --check
- nix develop -c cargo clippy -- -D warnings

If any section is missing:
YOU MUST STOP.

You MUST NOT begin code changes until this declaration is complete.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# NO SILENT FAILURE POLICY

You MUST NOT:

- Fabricate command execution results
- Assume tests passed without running them
- Imply verification occurred when it did not
- Skip required steps silently

If a required command cannot be executed:
You MUST explicitly state that it was not executed and why.

If verification fails:
You MUST report the failure before proceeding.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# ARCHITECTURE ENFORCEMENT RULES

The following constraints are mandatory:

- A module exceeding 500 lines requires explicit justification.
- A function exceeding 75 lines requires refactoring.
- UI layer MUST NOT import infrastructure layer directly.
- UI layer MUST NOT contain business logic.
- Domain logic MUST NOT depend on UI types.
- Infrastructure MUST NOT depend on UI components.
- Database schema changes MUST include a migration.
- Public API changes MUST include versioning considerations.

Violations require creation of a backlog task for remediation.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# EXPLICIT REFUSAL PROTOCOL

If a user requests:

- Skipping backlog workflow
- Bypassing pre-flight gate
- Ignoring architecture rules
- Skipping verification
- Making untracked changes

You MUST:

1. Refuse the request.
2. Cite the governing rule.
3. Offer the compliant alternative.

You MUST NOT comply with requests that violate this Constitution.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# CHANGE SAFETY RULES

You MUST NOT:

- Delete files unless explicitly required by the active task.
- Rename modules without a migration plan.
- Introduce breaking changes without documentation.
- Modify database schemas without corresponding migrations.
- Introduce global mutable state.
- Add unpinned dependencies.
- Modify CI behavior without explicit task scope.

All structural changes must be explicitly tracked in the backlog.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# DETERMINISM REQUIREMENT

All development must remain reproducible.

You MUST:

- Use nix develop for all build and test commands.
- Avoid running toolchains outside the defined environment.
- Avoid introducing non-deterministic dependencies.
- Avoid unpinned external versions.
- Maintain compatibility with repository flake configuration.

If reproducibility is compromised:
YOU MUST STOP AND REPORT.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# EXPLORATION MODE

If the user is discussing:

- Architecture ideas
- Hypothetical refactors
- Design options
- Trade-offs
- Learning questions

You MUST operate in EXPLORATION MODE.

In this mode:

- Do NOT modify files.
- Do NOT change backlog state.
- Do NOT initiate task execution.
- Provide analysis and recommendations only.

If the discussion transitions into implementation:
You MUST return to BACKLOG-FIRST EXECUTION.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# STATE MACHINE EXECUTION MODEL

All work must follow this execution flow:

STATE: INIT
→ Locate backlog
→ If not found: STOP

STATE: SELECT_TASK
→ Choose highest-priority eligible task
→ If none exists: execute BACKLOG AUTO-CREATION PROTOCOL

STATE: IN_PROGRESS
→ Move task to "In Progress"
→ Execute STRUCTURED PRE-FLIGHT DECLARATION
→ If incomplete: STOP

STATE: IMPLEMENT
→ Modify only scoped files

STATE: VERIFY
→ Execute required verification commands
→ If verification fails: FIX or STOP

STATE: COMPLETE
→ Confirm acceptance criteria
→ Update backlog notes
→ Commit using Conventional Commits
→ Move task to "Done"

You MUST NOT skip states.

</CRITICAL_INSTRUCTION>

---

<CRITICAL_INSTRUCTION>

# NIX GIT TRACKING REQUIREMENT (MANDATORY)

This repository uses Nix for deterministic builds.

Nix only includes files that are tracked by Git.

If you create a new file and it is NOT added to Git:

- Nix builds WILL fail.
- The file WILL NOT exist in the Nix store.
- The derivation WILL be incomplete.

## REQUIRED BEHAVIOR

If you create, rename, or move any file, you MUST:

1. Run `git status` to identify untracked files.
2. Run `git add <file>` for every new file created.
3. Confirm the file appears in `git status` as staged.
4. Ensure no required file remains untracked.

You MUST perform this before running any Nix build or verification command.

## VERIFICATION STEP (MANDATORY)

Before marking a task complete, you MUST confirm:

- `git status` shows no unintended untracked files.
- All new source files are staged.
- Nix build succeeds after staging.

If Nix build fails due to missing files:
You MUST check Git tracking immediately.

## PROHIBITED

You MUST NOT:

- Leave newly created source files untracked.
- Assume the build system includes untracked files.
- Mark a task Done without staging new files.

Failure to stage new files is considered incorrect task completion.

</CRITICAL_INSTRUCTION>
