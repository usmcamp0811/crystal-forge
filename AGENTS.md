# Crystal Forge Agent Guide

This file governs work performed by automated agents in this repository. Follow higher-priority platform and safety instructions first. When this file, an active task, and a user request disagree, stop before making changes and ask the user to resolve any conflict that materially affects scope or behavior.

## Start by classifying the request

| Request                         | Backlog task                                      | Dedicated worktree | Repository writes                       |
| ------------------------------- | ------------------------------------------------- | ------------------ | --------------------------------------- |
| Explain, answer, or explore     | Not required                                      | Not required       | No                                      |
| Review or diagnose              | Not required                                      | Not required       | No, unless the user also asks for a fix |
| Maintain or groom the backlog   | No implementation task required                   | Not required       | Backlog-related files only              |
| Implement a change              | Required and `To Do`                              | Required           | Active-task scope only                  |
| Work on the next available task | Select the highest-priority eligible `To Do` task | Required           | Active-task scope only                  |

Do not change files, backlog state, branches, or merge requests for a read-only request. If a request changes from analysis to implementation, perform the implementation preflight before writing.

## Core rules

1. Follow the user's requested task. Do not substitute a different task merely because it has a higher backlog priority. Select the highest-priority task only when the user explicitly asks for the next available work.
2. Do not modify application code without an active, sprint-ready backlog task in `To Do`.
3. Use one dedicated branch and worktree per implementation task. Never implement in the `main` or `dev` integration worktree.
4. Preserve user changes. Never discard, overwrite, or reformat unrelated work.
5. Keep changes within the active task's acceptance criteria. Record unrelated discoveries as new `Backlog` tasks; do not implement them unless the user expands the scope.
6. Follow existing repository patterns before introducing new abstractions.
7. Run verification appropriate to the affected behavior. Never claim a command passed unless it was run and its actual exit status was successful.
8. Use the repository's Nix development environment for project toolchains and verification.
9. Do not merge an MR unless the user explicitly authorizes it.
10. Ask before making a decision that materially changes public behavior, compatibility, persistence, security boundaries, architecture, or task scope.
11. Don't rely on utilities to be installed like python or glab.. just use `nix run nixpkgs#glab` for these type of things
12. Treat source documentation as part of correctness. A behavior change is incomplete when its affected contracts, invariants, rationale, failure behavior, or other required documentation are stale or missing.

## Repository architecture

Crystal Forge is a Nix flake containing Rust server, agent, and builder components, a Dioxus WASM frontend, PostgreSQL persistence through SQLx, and Nix/Playwright integration checks.

Preserve these boundaries:

- The server owns persistence, authorization, job coordination, and server-side domain policy.
- API-only builders do not access the Crystal Forge database directly.
- Builder sessions and server-issued job authorization must remain enforced at API boundaries.
- UI views compose presentation and interaction. Put reusable state transitions and domain decisions outside view markup.
- Browser/WASM code must use browser-compatible APIs. Do not assume native `std::time`, filesystem, process, or socket behavior is available in WASM.
- Database schema changes require migrations. SQLx compile-time metadata must match schema and query shapes.
- Maintain compatibility with supported deployed agents/builders unless the active task explicitly defines a breaking transition.
- Treat bootstrap signing, session validation, cache verification, derivation transport, secret redaction, and authorization checks as security-sensitive code.
- Design documents named by an active task are authoritative for that task. Existing behavior and tests are evidence, but do not silently override an explicit design requirement.

Avoid arbitrary abstractions. Introduce a trait, shared component, or new layer only when it expresses a real boundary, enables needed testing, or matches an established repository pattern. Do not refactor solely to satisfy a line-count target.

## Backlog workflow

Use the Backlog.md MCP integration when available; otherwise use the repository-provided Backlog CLI from `nix develop`. See [docs/agent/backlog-workflow.md](docs/agent/backlog-workflow.md).

The valid lifecycle is:

```text
Backlog -> To Do -> In Progress -> Review -> Done
```

- New discoveries default to `Backlog`.
- Only a human selects work for a sprint by moving `Backlog` to `To Do`, unless the user explicitly delegates that decision.
- `In Progress` requires a task lock and dedicated worktree.
- `Review` requires an open MR and completed verification. During this time the MR will be deployed to a test server and any changes requested must be done and will result in new database migrations NOT! edits to existing mirations.
- `Done` requires the MR to be merged and the task worktree to be removed.

## Implementation preflight

Before the first implementation write:

1. Resolve the user-requested task and read its acceptance criteria, non-goals, dependencies, risk, and verification plan.
2. Confirm it is sprint-ready, in `To Do`, and has no active lock.
3. Discover the integration worktrees and verify `main` and `dev` are clean.
4. Create a task branch and worktree from the designated integration branch, normally `dev`.
5. Verify the new worktree, branch, base, and status.
6. Move the task to `In Progress` and add its lock.
7. State a concise preflight containing the task, worktree, branch/base, intended scope, and verification plan.

Do not pretend that `git status` in one worktree proves another worktree is clean. Exact commands and recovery rules are in [docs/agent/worktrees.md](docs/agent/worktrees.md).

## Implementation standards

### Source documentation and technical writing

These rules apply to source comments, Rust documentation comments, architecture notes, design notes, task notes, MR descriptions, user-facing technical text, and other technical prose created or modified by an agent.

The key words **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** in this section are normative and are interpreted as described by [IETF BCP 14](https://www.rfc-editor.org/info/bcp14), [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119), and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174).

Agents MUST use these published standards as the baseline instead of inventing an ad hoc documentation style:

- [The Rust Style Guide](https://doc.rust-lang.org/style-guide/) controls Rust source and comment formatting.
- [The rustdoc Book](https://doc.rust-lang.org/rustdoc/) controls Rust API documentation behavior and doctests.
- [Rust RFC 1574](https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html) controls Rust API documentation conventions.
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/documentation.html) controls expectations for complete and useful Rust public API documentation.
- [ASD-STE100 Simplified Technical English, Issue 9](https://www.asd-ste100.org/) controls technical prose style, subject to necessary Rust, Nix, SQL, protocol, product, and Crystal Forge terminology.
- [ISO/IEC/IEEE 42010:2022](https://www.iso.org/standard/74393.html) applies when an agent creates or materially changes architecture descriptions. Source comments MUST NOT be used as a substitute for required architecture documentation.

Repository-specific rules below are intentionally stricter when needed. Do not interpret any cited guideline as permission to weaken an explicit rule in this file.

#### Documentation objective

Code MUST communicate its intended semantics, not only its mechanics.

A competent maintainer MUST be able to determine the contract and the reasons for non-obvious behavior without reverse-engineering unrelated code, database constraints, tests, historical commits, or author intent.

When applicable, documentation MUST make these concepts explicit:

- contracts, preconditions, postconditions, and caller obligations;
- invariants and properties that future changes must preserve;
- state meanings, valid state transitions, terminal states, and forbidden transitions;
- side effects and significant operations that intentionally do not occur;
- error conditions, retry behavior, cancellation behavior, and partial-failure behavior;
- concurrency, ownership, locking, ordering, atomicity, and race-prevention assumptions;
- persistence behavior, transaction boundaries, uniqueness assumptions, and data-integrity requirements;
- authorization, trust boundaries, security assumptions, redaction requirements, and sensitive-data handling;
- external protocol, serialization, compatibility, versioning, and deployed-component assumptions;
- units, ranges, thresholds, timeouts, sentinel values, and the reason for arbitrary-looking constants;
- non-obvious business rules and domain semantics;
- non-obvious performance tradeoffs or deliberate optimizations;
- `unsafe` requirements and the local proof that each unsafe operation is valid.

Do not rely on tests as the only description of an intended contract. Tests prove observed behavior. Documentation identifies which behavior is intentional and why.

Do not use comment count, comment-to-code ratio, or documentation volume as a quality target. A comment that only restates identifiers, syntax, or immediately visible control flow SHOULD NOT exist.

#### Required Rust documentation

Every public item added or materially modified by a change MUST have useful rustdoc documentation. This includes crates, modules, structs, enums, traits, public fields, functions, methods, associated items, constants, statics, type aliases, and macros. Existing lint configuration does not waive this requirement.

An agent MUST NOT add `allow(missing_docs)` or another suppression to avoid documenting code.

Crate-level and module-level documentation MUST explain responsibility, boundaries, and important invariants when those facts are not already obvious from the surrounding architecture.

Public type documentation MUST explain the semantic meaning of the type and any invariant that valid values maintain.

Public enum documentation MUST explain the meaning of states or variants when they represent a lifecycle, protocol, status, policy, or domain state. Document valid and forbidden transitions when transition rules exist.

Public function and method documentation MUST state the observable contract. Include the following sections when applicable:

- `# Errors` for meaningful error conditions returned by fallible APIs.
- `# Panics` for caller-reachable panic conditions.
- `# Safety` for every `unsafe fn` and unsafe trait contract.
- `# Examples` for non-trivial public APIs. Examples SHOULD be doctests and MUST remain valid when behavior changes.
- `# Aborts` or `# Undefined Behavior` when those conditions are relevant.

The first rustdoc sentence MUST be a concise summary and SHOULD use the RFC 1574 third-person singular present form, for example, `Returns`, `Creates`, or `Schedules`.

Use intra-doc links for related Rust items when they improve understanding. Broken documentation links are defects.

#### Required maintainer-facing comments

Private code does not need comments merely because it is private. It MUST be documented when correct maintenance depends on information that cannot be reliably inferred from local names, types, and control flow.

The following implementation concepts MUST have adjacent explanatory documentation when they occur:

- concurrency control, lock ordering, worker ownership, deduplication, and race prevention;
- transaction boundaries and multi-step persistence invariants;
- security or authorization decisions whose placement or ordering matters;
- retry, idempotency, recovery, lease, heartbeat, timeout, or cancellation semantics;
- state-machine transitions and restrictions not enforced completely by the type system;
- compatibility paths, legacy formats, deployed-version assumptions, and temporary workarounds;
- algorithms whose correctness depends on a non-obvious invariant;
- non-obvious performance optimizations or query-shape choices;
- domain rules that would otherwise have to be inferred from implementation behavior;
- arbitrary-looking constants, thresholds, limits, durations, or ordering choices whose source or constraint is external to the local expression;
- deliberate lint suppressions or unusual language/toolchain workarounds.

Use these labels consistently when they make the semantic category clearer:

```rust
// INVARIANT:
// CONCURRENCY:
// SAFETY:
// SECURITY:
// COMPATIBILITY:
// PERFORMANCE:
```

The text after a label MUST state the property or rationale that must remain true. Do not add a label to an obvious comment solely to satisfy this rule.

Every significant `unsafe` block MUST have an immediately preceding `// SAFETY:` comment that explains why the unsafe operation is valid at that exact location. The comment MUST identify the relevant lifetime, aliasing, initialization, bounds, ownership, thread-safety, FFI, or other safety invariant. Writing only what the unsafe expression does is insufficient.

#### Comment and rustdoc formatting

Rust comments MUST follow the current Rust Style Guide.

In particular:

- Prefer `//` to `/* ... */`.
- Prefer `///` to `/** ... */`.
- Use `//!` only for crate-level or module-level documentation.
- Put ordinary comments on their own line unless an inline comment is substantially clearer.
- Put one space after `//`, `///`, or `//!`.
- Write ordinary comments as complete sentences unless a short annotation is clearer.
- Start a normal sentence with a capital letter and end it with punctuation.
- Limit lines that consist entirely of comments to 80 characters where the Rust Style Guide permits; never exceed the repository's normal Rust line-width rules.
- Put rustdoc before item attributes as required by the Rust Style Guide.
- Use Markdown in rustdoc according to RFC 1574 and rustdoc conventions.

Do not commit commented-out production code. Version control retains removed implementations.

`TODO` and `FIXME` comments MUST identify a tracked task unless the comment will be resolved in the same change. They MUST state the specific remaining condition, not only `fix this` or equivalent.

Example:

```rust
// TODO(TASK-482): Remove the version-2 compatibility path after all
// supported agents advertise protocol version 3.
```

#### Technical writing standard

Technical prose MUST follow ASD-STE100 principles to the extent that they are compatible with exact software terminology.

Agents MUST:

- use short, direct, declarative sentences;
- use active voice when the actor is known and active voice improves clarity;
- use one term consistently for one concept instead of rotating through synonyms;
- use exact identifiers, state names, units, boundaries, and conditions;
- make cause and effect explicit;
- make required, prohibited, optional, and conditional behavior explicit;
- use project and domain terms consistently with the code and design documents;
- prefer specific nouns over vague references such as `this`, `that`, `it`, or `they` when the referent could be ambiguous;
- separate distinct requirements or actions instead of joining unrelated ideas in one long sentence;
- state negative guarantees when they are part of the contract, such as operations that MUST NOT enqueue work, mutate state, reveal existence, or retry;
- keep prose concise without omitting information needed for safe maintenance.

Agents MUST NOT use filler or vague technical prose such as `it is important to note`, `basically`, `simply`, `as needed`, `appropriately`, `properly`, or `handle this` unless the sentence defines the exact condition or behavior.

Project names, Rust identifiers, SQL identifiers, API names, protocol terms, security terms, and other necessary subject-specific vocabulary are permitted even when they are not in the ASD-STE100 general dictionary.

#### Documentation maintenance and review gate

Documentation is part of the implementation, not a follow-up activity.

When behavior changes, the same change MUST update every affected source comment, rustdoc contract, architecture note, design document, example, and tracked technical description within the active task's scope.

Stale documentation is a correctness defect. An agent MUST remove or correct a comment that no longer describes the resulting code.

Before reporting implementation ready for review, the agent MUST inspect the diff specifically for documentation quality and confirm:

- every new or materially changed public Rust item is documented;
- required `# Errors`, `# Panics`, `# Safety`, and `# Examples` sections are present where applicable;
- every significant unsafe block has a valid local `SAFETY` rationale;
- non-obvious concurrency, persistence, security, compatibility, state, and performance semantics are documented;
- no comment merely paraphrases obvious code;
- no stale comment remains after implementation changes;
- no untracked `TODO` or `FIXME` was introduced;
- rustdoc examples and links affected by the change are verified when practical.

If the repository already provides documentation lints or rustdoc checks, run them. For Rust API changes, run the applicable rustdoc build or doctest command through the repository's Nix environment when it is needed to verify the changed documentation. Do not modify repository-wide lint policy solely to make an unrelated task pass.

### Rust

- Use `Result`-based error handling and preserve useful error context.
- Do not use `unwrap` or `expect` on reachable production error paths.
- Avoid unnecessary cloning and blocking work in async execution paths.
- Keep API models, domain decisions, persistence, and transport concerns separated according to existing modules.
- Use the SQLx query form that best matches the query and surrounding repository conventions.

### Dioxus/WASM

- Keep rendering code focused on presentation and event wiring.
- Extract nontrivial state transitions and test them independently when practical.
- Keep client DTOs aligned with the server contract, but do not duplicate server types when the UI intentionally needs a different representation.
- Preserve loading, empty, error, authorization, and stale-data behavior when changing views.
- A user-visible UI change must be exercised by the authoritative `web-ui` check and represented by an MR screenshot. Add a behavioral assertion when practical.

### Database and SQLx

- Add a migration for every schema change; never edit an already-applied migration unless repository policy explicitly permits it.
- Update SQLx offline metadata for changes to migrations, checked queries, selected columns, bind parameters, or query result shapes.
- Perform destructive database reset/refresh operations only against a verified isolated local development database started by this repository.
- Never use a shared, staging, production, or default local PostgreSQL instance for SQLx preparation.

See [docs/agent/database-safety.md](docs/agent/database-safety.md).

## Verification

Choose the smallest set of commands that proves the acceptance criteria and protects affected interfaces. Prefer targeted checks during implementation and broader checks before review when risk warrants them.

Use the exact package manifests and flake attributes applicable to the change. The baseline command matrix is in [docs/agent/verification.md](docs/agent/verification.md).

Run `nix flake check --keep-going` when the task requires it or the change affects flakes, NixOS modules, development shells, packaging, CI/release behavior, or cross-package interfaces that targeted checks cannot adequately prove. Do not run it mechanically for every edit.

If a required check cannot run, report the command, failure, and impact. Do not move the task to `Review` while required verification is incomplete.

Output summarizers such as `distill` are optional reading aids, never proof. Preserve and inspect the underlying command's real exit status. Do not summarize output that must be retained verbatim or parsed mechanically.

## Scope and safety

- Inspect freely within the repository when needed to understand the task.
- Make reasonable, reversible, repository-consistent assumptions when they do not change scope or public behavior.
- Do not delete files, rename public modules, add dependencies, change CI, or introduce breaking API/schema behavior unless the task requires it.
- Do not expose secrets, authorization headers, credentials, signed URLs, or sensitive environment data in logs, tests, task notes, commits, or MR descriptions.
- Do not use destructive Git commands to resolve an unexpected dirty worktree.
- If verification reveals an unrelated defect, create a Backlog task and continue only if the active task remains safely verifiable.

Stop and ask the user when acceptance criteria conflict, a required dependency is unavailable, the correct migration or compatibility strategy is ambiguous, a destructive operation cannot be proven local and isolated, or continuing would overwrite someone else's work.

## Review and completion

Before opening an MR:

- Confirm every acceptance criterion.
- Run the declared verification and record exact commands and outcomes.
- Confirm only intended files changed and all intended new files are tracked.
- Update SQLx metadata when applicable.
- Add MR screenshots for user-visible UI changes.
- Record out-of-scope discoveries as Backlog tasks.
- Confirm source documentation and technical prose satisfy the documentation standard above; undocumented required semantics or stale documentation block review.

Then open the MR, move the task to `Review`, add the MR link to the task, and remove the lock or mark it as awaiting review according to Backlog.md conventions.

After the MR is merged, remove the task worktree, prune stale worktree metadata if necessary, and move the task to `Done`. Do not report the task as complete merely because implementation was pushed.

## Reporting

Be precise and concise. Distinguish among:

- implemented but not verified;
- verified locally;
- pushed to a branch;
- open for review;
- merged and complete.

Never fabricate command output, test results, task state, commits, pushes, MR state, or screenshots.
