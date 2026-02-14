
<!-- BACKLOG.MD MCP GUIDELINES START -->

<CRITICAL_INSTRUCTION>

## BACKLOG WORKFLOW INSTRUCTIONS

This project uses Backlog.md MCP for all task and project management activities.

**CRITICAL GUIDANCE**

- If your client supports MCP resources, read `backlog://workflow/overview` to understand when and how to use Backlog for this project.
- If your client only supports tools or the above request fails, call `backlog.get_workflow_overview()` tool to load the tool-oriented overview (it lists the matching guide tools).

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

<!-- BACKLOG.MD MCP GUIDELINES END -->

---

# 🤖 AI Agent Onboarding Guide

Welcome to the Crystal Forge project! This document will help you understand the project structure, workflows, and best practices.

## 📋 Table of Contents

1. [Project Overview](#project-overview)
2. [Engineering Standards (TDD & Quality)](#engineering-standards-tdd--quality)
3. [Task Management Workflow](#task-management-workflow)
4. [Git Workflow](#git-workflow)
5. [Development Environment](#development-environment)
6. [Testing Strategy](#testing-strategy)
7. [Common Pitfalls & Solutions](#common-pitfalls--solutions)
8. [Quick Reference](#quick-reference)

---

## 🎯 Project Overview

**Crystal Forge** is a NixOS fleet management system that enables:
- Centralized management of NixOS systems
- Automated deployments with rollback capabilities
- Build orchestration and caching
- CVE scanning and security monitoring

**Tech Stack**:
- **Language**: Rust (edition 2024, rustc 1.91.1)
- **Database**: PostgreSQL (with SQLx, compile-time verified queries)
- **Build System**: Nix (Snowfall Lib, nixpkgs release-25.11)
- **Web UI**: Dioxus 0.7 (WASM target, Tailwind CSS via CDN)
- **Package Manager**: Cargo
- **Task Management**: Backlog.md CLI

**Project Structure** (key directories):
```
crystal-forge/
├── packages/default/          # Server, agent, builder, keygen (Rust)
│   ├── src/
│   │   ├── api/models.rs      # Shared API DTOs
│   │   ├── handlers/api/      # REST API handlers (dashboard, etc.)
│   │   ├── builder/           # Build orchestration
│   │   └── deployment/        # Deployment logic
│   └── .sqlx/                 # SQLx offline query cache
├── packages/web-ui/           # Dioxus 0.7 web UI (WASM)
│   ├── src/
│   │   ├── api/               # Client-side DTOs + fetch client
│   │   ├── components/        # Reusable UI components
│   │   ├── views/             # Route-level page components
│   │   ├── state/             # App state with context provider
│   │   ├── theme.rs           # Design system tokens
│   │   └── routes.rs          # Dioxus Router enum
│   ├── Dioxus.toml            # Dioxus build config
│   └── Cargo.lock             # Separate lockfile (WASM deps)
├── checks/                    # Nix flake checks (CI verification)
│   ├── server/                # Server integration tests (NixOS VM)
│   ├── web-ui/                # Web UI build verification
│   └── database/              # Database migration tests
├── backlog/                   # Task management (Backlog.md)
├── flake.nix                  # Nix flake entry point
└── CLAUDE.md                  # This file
```

---

## 🏗️ Engineering Standards (TDD & Quality)

### Test-Driven Development

Every change MUST be verified before it is considered complete. This is non-negotiable.

**Per-crate test expectations**:

| Crate | Verification Command | Notes |
|-------|---------------------|-------|
| `packages/default` (lib) | `nix develop -c bash -c "cd packages/default && SQLX_OFFLINE=true cargo test --lib"` | 35+ unit tests, SQLx offline mode |
| `packages/default` (server) | `nix develop -c bash -c "cd packages/default && SQLX_OFFLINE=true cargo check --bin server"` | Compile check (full test needs DB) |
| `packages/web-ui` | `nix build .#checks.x86_64-linux.web-ui` | WASM build + output validation |
| Full integration | `nix build .#checks.x86_64-linux.server` | NixOS VM test (slow, ~10min) |

**Rules**:
1. **New features MUST have corresponding tests** — or at minimum, the existing test suite must pass
2. **Run verification before marking any task as Done** — no exceptions
3. **If a check fails, fix it before moving on** — don't leave broken builds
4. **Always `git add` new files immediately** — Nix flake checks only see git-tracked files

### Code Quality Standards

- Use `cargo clippy` for linting (in nix develop shell)
- Use `cargo fmt` for formatting
- Prefer `sqlx::query_as` over `sqlx::query!` for new queries (avoids compile-time DB dependency)
- Keep web-ui DTOs in sync with server `api/models.rs` types

---

## 📊 Task Management Workflow

### **CRITICAL**: Always Update Task Status

When working on tasks, you MUST follow this lifecycle:

```
To Do → In Progress → Done
```

### Commands

```bash
# List all tasks
backlog task list

# View task details
backlog task view TASK-X.Y

# Start working on a task (move to "In Progress")
backlog task edit TASK-X.Y -s "In Progress"

# Mark task as complete
backlog task edit TASK-X.Y -s "Done"

# Add notes to a task
backlog task edit TASK-X.Y --append-notes "Completed implementation of X"

# Check acceptance criteria
backlog task edit TASK-X.Y --check-ac 1
```

### Valid Status Values

- `"To Do"` - Task is planned but not started
- `"In Progress"` - Task is actively being worked on
- `"Done"` - Task is complete

**Note**: Status values are case-sensitive and must be quoted if they contain spaces!

### **CRITICAL**: Handling Out-of-Scope Work

> [!IMPORTANT]
> **When you discover work that needs to be done but is outside the scope of your current task:**
>
> 1. **DO NOT** immediately start working on it
> 2. **DO** create a new task in the backlog
> 3. **DO** continue with your current task
> 4. **DO** reference the new task in your notes if relevant
>
> **Example**: While fixing a deployment bug, you notice a typo in documentation:
> ```bash
> # Create a new task for the typo
> backlog task create "Fix typo in deployment documentation" \
>   -d "Found typo in docs/deployment.md line 42 while working on TASK-1.6" \
>   -l documentation,typo \
>   --priority low
> 
> # Add note to current task
> backlog task edit TASK-1.6 --append-notes "Created TASK-X for documentation typo"
> 
> # Continue with current task
> ```
>
> **Why this matters**:
> - Keeps work focused and traceable
> - Prevents scope creep
> - Ensures nothing is forgotten
> - Maintains clean git history (one task = one branch)

### Task Update Checklist

Before ending a work session, ensure you:

- [ ] Move started tasks to "In Progress"
- [ ] Move completed tasks to "Done"
- [ ] Check off completed acceptance criteria
- [ ] Add implementation notes if relevant
- [ ] Update dependencies if tasks are blocked

---

## 🔀 Git Workflow

### Branch Strategy

```
main (production)
  └── refactor (base for refactoring work)
       ├── fix/deployment-persistence
       ├── feat/service-layer
       └── refactor/builder-decomposition
```

### Branch Naming Convention

- `fix/` - Bug fixes
- `feat/` - New features
- `refactor/` - Code refactoring
- `docs/` - Documentation updates
- `test/` - Test additions/improvements

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>: <short description>

<detailed description>

Closes: TASK-X.Y, TASK-X.Z
```

**Types**: `fix`, `feat`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`

**Example**:
```bash
git commit -m "fix: add configurable deployment strategy with generation creation

- Add DeploymentStrategy enum (ImmediatePersist, BootOnly)
- Default to ImmediatePersist (create generation + activate immediately)
- Refactor activate_configuration to create NixOS generation first
- Add helper methods: create_generation, verify_generation_created, activate_via_systemd

This ensures agent deployments persist across reboots by creating proper
NixOS generations using nix-env --profile /nix/var/nix/profiles/system --set.

Closes: TASK-1.1, TASK-1.2, TASK-1.3, TASK-1.4, TASK-1.5"
```

### Workflow Steps

1. **Create feature branch** (one branch per task):
   ```bash
   git checkout refactor
   git pull origin refactor
   git checkout -b feat/your-feature-name
   ```

2. **Always `git add` new files immediately** after creating them:
   ```bash
   # Nix flake checks only see git-tracked files!
   git add packages/web-ui/src/new_file.rs
   ```

3. **Make changes and commit**:
   ```bash
   git add <files>
   git commit -m "type: description"
   ```

4. **Run verification** before considering work done:
   ```bash
   # See "Engineering Standards" section for per-crate commands
   nix develop -c bash -c "cd packages/default && SQLX_OFFLINE=true cargo test --lib"
   nix build .#checks.x86_64-linux.web-ui
   ```

5. **STOP for review** — do NOT merge to `refactor` without user approval:
   ```bash
   git push origin feat/your-feature-name
   # Wait for user to review the branch
   ```

6. **Merge back to refactor** (only after user approves):
   ```bash
   git checkout refactor
   git merge feat/your-feature-name
   git push origin refactor
   ```

---

## 🛠️ Development Environment

### Nix Development Shell

**ALWAYS** use the Nix development shell for build commands:

```bash
# Enter the development shell
nix develop

# Or run a single command
nix develop -c bash -c "cd packages/default && cargo test"
```

### Why Nix?

- Provides all required dependencies (OpenSSL, pkg-config, etc.)
- Ensures reproducible builds
- Manages PostgreSQL and other services

### Common Commands

```bash
# Start all services (PostgreSQL + Server + Agent)
full-stack up

# Start just PostgreSQL + Server
server-stack up

# Start just PostgreSQL
db-only up

# Run the agent
run-agent

# Run agent with local code (development)
run-agent --dev

# Run the server
run-server

# Refresh database and SQLx metadata
sqlx-refresh

# Just regenerate SQLx metadata
sqlx-prepare
```

---

## 🧪 Testing Strategy

### SQLx Compile-Time Verification

**CRITICAL**: This project uses SQLx with compile-time query verification.

**What this means**:
- `cargo test` and `cargo build` require a running PostgreSQL database
- SQLx macros (`sqlx::query!`) verify SQL against the actual schema at compile time

**Solutions**:

1. **Start the database first**:
   ```bash
   nix develop
   db-only up
   sqlx-prepare
   cargo test
   ```

2. **Use offline mode** (if `.sqlx/` directory exists):
   ```bash
   SQLX_OFFLINE=true cargo test
   ```

3. **Just check syntax** (no database required):
   ```bash
   cargo check
   ```

### Test Types

1. **Unit Tests**: Test individual functions/modules
   ```bash
   cargo test --lib
   ```

2. **Integration Tests**: Test full workflows
   ```bash
   cargo test --test integration_tests
   ```

3. **Manual Tests**: System-level changes (deployments, etc.)
   - Document procedures in walkthrough.md
   - Require real NixOS hardware (not VMs)

---

## ⚠️ Common Pitfalls & Solutions

### 1. Whitespace Matching in File Edits

**Problem**: `replace_file_content` fails with "target content not found"

**Solution**:
```bash
# Check exact whitespace
sed -n 'START,ENDp' file.rs | cat -A

# Tabs show as ^I, spaces as ·
# Ensure exact character-for-character match
```

### 2. SQLx Database Connection Errors

**Problem**: `error communicating with database: Connection refused`

**Solution**:
```bash
# Start PostgreSQL
nix develop
db-only up

# Regenerate SQLx metadata
sqlx-prepare
```

### 3. OpenSSL Build Failures

**Problem**: `Could not find directory of OpenSSL installation`

**Solution**:
```bash
# Always use nix develop
nix develop -c bash -c "cargo build"

# Don't run cargo directly in host environment
```

### 4. Standalone Test Files

**Problem**: Creating `.rs` files outside project structure fails

**Solution**:
- Work within existing project structure
- Use `cargo test` with proper dependencies
- Don't try to compile standalone files with external crates

### 5. Task Status Updates

**Problem**: Forgetting to update task status

**Solution**:
- Set a reminder to update tasks before ending work session
- Use this checklist:
  - [ ] Move started tasks to "In Progress"
  - [ ] Move completed tasks to "Done"
  - [ ] Add implementation notes
  - [ ] Check acceptance criteria

---

## 📚 Quick Reference

### File Locations

```
crystal-forge/
├── backlog/                    # Task management
│   ├── README.md              # Backlog overview
│   ├── tasks/                 # Individual task files
│   └── milestones/            # Milestone documents
├── packages/default/          # Main Rust package (server, agent, builder)
│   ├── src/
│   │   ├── api/models.rs      # Shared API DTOs
│   │   ├── handlers/api/      # REST API handlers
│   │   ├── builder/           # Build orchestration
│   │   ├── deployment/        # Deployment logic
│   │   └── config/            # Configuration
│   └── Cargo.toml
├── packages/web-ui/           # Dioxus 0.7 web UI (WASM)
│   ├── src/
│   │   ├── api/               # Client DTOs + fetch client
│   │   ├── components/        # Reusable UI components
│   │   ├── views/             # Page components
│   │   ├── state/             # App state management
│   │   ├── theme.rs           # Design system tokens
│   │   └── routes.rs          # Router definitions
│   ├── Dioxus.toml            # Dioxus build config
│   └── Cargo.lock             # Separate lockfile
├── checks/                    # Nix flake checks
│   ├── server/                # Server integration (NixOS VM)
│   ├── web-ui/                # Web UI build verification
│   └── database/              # DB migration tests
├── CLAUDE.md                  # This file (AI onboarding)
├── ROADMAP.md                 # Project roadmap
└── flake.nix                  # Nix configuration
```

### Backlog CLI Cheat Sheet

```bash
# Task Management
backlog task list                           # List all tasks
backlog task view TASK-X.Y                  # View task details
backlog task edit TASK-X.Y -s "In Progress" # Update status
backlog task edit TASK-X.Y --check-ac 1     # Check acceptance criterion
backlog task edit TASK-X.Y --append-notes "..." # Add notes

# Milestone Management
backlog milestone list                      # List milestones
backlog milestone view MILESTONE-NAME       # View milestone

# Search
backlog search "keyword"                    # Search tasks/milestones
```

### Git Cheat Sheet

```bash
# Branch Management
git checkout -b fix/feature-name            # Create feature branch
git checkout refactor                       # Switch to refactor branch
git merge fix/feature-name                  # Merge feature branch

# Commit
git add <files>
git commit -m "type: description"
git push origin branch-name

# View Status
git status                                  # Check working directory
git log --oneline -10                       # View recent commits
git branch -a                               # List all branches
```

### Nix Cheat Sheet

```bash
# Development Shell
nix develop                                 # Enter dev shell
nix develop -c bash -c "command"            # Run single command

# Services
full-stack up                               # Start all services
db-only up                                  # Start PostgreSQL only
run-agent                                   # Run agent
run-server                                  # Run server

# Database
sqlx-refresh                                # Drop DB + regenerate metadata
sqlx-prepare                                # Regenerate metadata only
```

### Cargo Cheat Sheet

```bash
# Building
cargo build                                 # Debug build
cargo build --release                       # Release build
cargo check                                 # Check syntax (no DB needed)

# Testing
cargo test                                  # Run all tests (needs DB)
cargo test --lib                            # Run library tests only
cargo test test_name                        # Run specific test

# Linting
cargo clippy                                # Run linter
cargo fmt                                   # Format code
```

---

## 🎓 Learning Resources

### Project-Specific Docs

- [Git Workflow](backlog/GIT_WORKFLOW.md) - Detailed Git workflow guide
- [Milestone: Critical Bugs & Stability](backlog/milestones/m-0%20-%20critical-bugs-and-stability.md) - Immediate focus (m-0)
- [Milestone: Development Infrastructure](backlog/milestones/m-1%20-%20development-infrastructure.md) - Dev/Test groundwork (m-1)
- [Milestone: Code Quality & Architecture](backlog/milestones/m-2%20-%20code-quality-and-architecture.md) - Core refactoring (m-2)
- [Milestone: User Interface](backlog/milestones/m-3%20-%20user-interface.md) - UI development (m-3)
- [Milestone: Advanced Features](backlog/milestones/m-4%20-%20advanced-features.md) - Deployment enhancements (m-4)
- [Task: Deployment Persistence](backlog/tasks/task-1%20-%20BUG-Agent-Deployments-Dont-Persist-Across-Reboots.md) - Example task (TASK-1)

### External Resources

- [Conventional Commits](https://www.conventionalcommits.org/)
- [Rust Book](https://doc.rust-lang.org/book/)
- [SQLx Documentation](https://github.com/launchbadge/sqlx)
- [Nix Manual](https://nixos.org/manual/nix/stable/)

---

## ✅ Pre-Work Checklist

Before starting work on any task:

- [ ] Read this onboarding guide
- [ ] Understand the backlog workflow (`backlog://workflow/overview`)
- [ ] Review the Git workflow ([GIT_WORKFLOW.md](backlog/GIT_WORKFLOW.md))
- [ ] Ensure you can enter the nix development shell (`nix develop`)
- [ ] Verify you can list tasks (`backlog task list`)
- [ ] Review the current milestone
- [ ] Check for any blocking dependencies

---

## 🚀 Post-Work Checklist

After completing work:

- [ ] Update task status to "Done" or "In Progress"
- [ ] Check acceptance criteria
- [ ] Add implementation notes to tasks
- [ ] **Create new tasks for any out-of-scope work discovered**
- [ ] Commit changes with proper commit message format
- [ ] Push changes to remote branch
- [ ] Update any related documentation
- [ ] Create walkthrough.md if significant changes were made
- [ ] Update lessons_learned.md with any mistakes/learnings

---

## 💡 Tips for Success

1. **Always use `nix develop`** - Don't run cargo/build commands outside the nix shell
2. **Update tasks frequently** - Don't wait until the end to update task status
3. **Follow commit conventions** - Makes history readable and searchable
4. **Document as you go** - Add notes to tasks, create walkthroughs for complex changes
5. **Learn from mistakes** - Update lessons_learned.md when you encounter issues
6. **Test thoroughly** - Unit tests + integration tests + manual tests when needed
7. **Ask for clarification** - Better to ask than make wrong assumptions
8. **Create tasks for out-of-scope work** - Don't let discoveries derail your current task; capture them in the backlog instead

---

## 🆘 Getting Help

If you encounter issues:

1. Check [lessons_learned.md](.gemini/antigravity/brain/*/lessons_learned.md) for similar issues
2. Review the backlog for related tasks
3. Check the Git history for similar changes
4. Read the relevant documentation in `backlog/`
5. Ask the user for clarification

---

**Remember**: This is a living document. Update it as you learn new patterns or encounter new issues!
