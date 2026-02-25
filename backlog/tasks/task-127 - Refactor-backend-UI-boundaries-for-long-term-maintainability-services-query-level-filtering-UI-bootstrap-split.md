---
id: TASK-127
title: >-
  Refactor backend/UI boundaries for long-term maintainability (services +
  query-level filtering + UI bootstrap split)
status: To Do
assignee: []
created_date: '2026-02-24 13:46'
updated_date: '2026-02-25 00:35'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
### Goal

Reduce handler/UI hotspot bloat and clarify architectural boundaries by introducing a lightweight service layer, moving list filtering/sorting/pagination into query functions, and splitting Web UI bootstrap concerns out of `web-ui/src/main.rs`. Keep behavior identical.

### Why (problems this solves)

- API handlers are mixing auth/RBAC + orchestration + filtering + DTO mapping, making changes risky.
- Some “models” perform persistence (calling queries), blurring domain vs storage boundaries.
- Systems listing work is done in-memory, which won’t scale and encourages duplicate logic.
- `web-ui/src/main.rs` is accumulating unrelated concerns (auth fetch, asset injection, bootstrapping).

### Non-goals

- No big rewrite or new framework.
- No behavior/UI changes.
- No new auth model or RBAC redesign.
- No database schema changes unless strictly required to support server-side filtering/sorting/pagination (and if required, keep minimal and justified).

### Scope

#### Backend (`packages/default`)

1. Add a service layer and move orchestration/policy out of handlers:
   - `src/services/mod.rs`
   - `src/services/systems.rs`
   - Optional (only if needed): `src/services/deployments.rs` (or similar)
2. Move systems list filtering/sorting/pagination into query functions:
   - Update `src/queries/systems.rs` (or relevant file)
3. Clarify model responsibilities:
   - Remove direct calls from `models/*` into `queries/*` where encountered (start with systems)
   - Replace with service functions called from handlers

#### Web UI (`packages/web-ui`)

4. Split `main.rs` into explicit bootstrap modules:
   - `src/bootstrap/mod.rs`
   - `src/bootstrap/auth.rs` (auth fetch / hydration)
   - `src/bootstrap/assets.rs` (CSS/script injection)
   - Keep `main.rs` thin: launch + root component composition

Test Code and Performance Tests
At this point, we are already writing a complete project. However, an essential part of any project is testing, including unit tests and performance tests. So where should these test files be placed? Let’s continue using our sdk project as an example.

According to community and official standards, test and benchmark files should be placed in tests and benches directories at the same level as src, as shown below:

sdk/
  ├── Cargo.toml
  ├── src/
  │   └── lib.rs
  ├── tests/
  │   ├── some-integration-tests.rs
  │   └── multi-file-test/
  │       ├── main.rs
  │       └── test_module.rs
  └── benches/
      ├── large-input.rs
      └── multi-file-bench/
          ├── main.rs
          └── bench_module.rs
When initially writing the project, unit tests can be placed directly below the relevant code files, so there is no need to create the multi-file-test directory and files. However, as development progresses and test code starts occupying significant space, it is recommended to move them to the tests folder to keep the main code clean.

tests/ contains functional test code, primarily for verifying feature implementation.
benches/ contains performance test code, primarily for measuring performance (e.g., service API performance tests).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `packages/default/src/handlers/api/systems.rs` no longer performs in-memory filtering/sorting/pagination; it delegates to `services::systems::list_systems_for_user(...)` (or equivalent).
- [ ] #2 `packages/default/src/queries/systems.rs` (or equivalent) supports server-side filters + sorting + pagination that match existing behavior.
- [ ] #3 No `packages/default/src/models/*` module calls into `packages/default/src/queries/*` directly for creation/insertion for the refactored domain (at least systems). Models are plain data + validation helpers only.
- [ ] #4 All existing endpoints return the same JSON schema and semantics as before.
- [ ] #5 Backend tests exist for systems list behavior:
- [ ] #6 At least one test covers “RBAC scoping + env membership filtering” for systems list (extend existing tests if present).
- [ ] #7 `packages/web-ui/src/main.rs` contains no auth-fetch logic and no raw asset injection logic.
- [ ] #8 Auth bootstrap is isolated in `packages/web-ui/src/bootstrap/auth.rs` and invoked from the root component or startup path.
- [ ] #9 Asset/script/style injection is isolated in `packages/web-ui/src/bootstrap/assets.rs`.
- [ ] #10 UI behavior is identical (including fallback/mock behavior if already implemented elsewhere).
- [ ] #11 `cargo test` passes for both crates.
- [ ] #12 `cargo fmt` is clean and `clippy` is clean (or matches current repo policy).
- [ ] #13 `nix flake check` (or repo check entrypoints) passes.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Part A — Backend service layer (systems)

1. Create services module:
   - Add `packages/default/src/services/mod.rs`
   - Export submodules, starting with:
     - `pub mod systems;`

2. Create systems service:
   - Add `packages/default/src/services/systems.rs`
   - Define service functions that represent use-cases, not SQL:
     - `list_systems_for_user(...)`
     - Optional (only if required by compilation/flow): `get_system_details(...)`, `create_system(...)`
   - Inputs should include:
     - auth context / user id
     - role/permission context
     - filter/sort/pagination options
   - Output should be either:
     - domain structs returned + mapping in handler, or
     - API DTOs returned directly
   - Pick one approach and be consistent within systems.

3. Move orchestration out of handler:
   - In `packages/default/src/handlers/api/systems.rs`, identify logic for:
     - auth extraction
     - membership/environment lookup
     - filtering/sorting/pagination
     - assembling response objects
   - Replace with a call into `services::systems::list_systems_for_user(...)`
   - Keep the handler responsible only for:
     - parsing request params
     - extracting auth/session info
     - calling the service
     - returning HTTP response

Definition of done (Part A): the handler is “skinny” and reads like a controller.

---

### Part B — Query-level filtering/sorting/pagination

4. Introduce explicit filter/sort/pagination types (small and focused):
   - Define in `services/systems.rs` or `queries/systems.rs`:
     - `SystemsListFilter` (only what the UI/handler already supports)
     - `SystemsSort` (field + direction)
     - `Pagination` (limit + offset OR page + page_size)

5. Implement query function(s):
   - In `packages/default/src/queries/systems.rs`, implement something like:
     - `list_systems_scoped(pool, scope, filter, sort, pagination) -> (Vec<SystemRow>, total_count)`
   - “scope” must implement existing RBAC/env membership rules:
     - admin sees all
     - operator/viewer sees only environments they’re in
     - match current behavior exactly

6. Remove in-memory filtering:
   - Ensure the service uses the query function for list behavior
   - Delete old in-memory filter/sort/pagination code from the handler

7. Performance sanity check:
   - Avoid N+1 queries in the list endpoint
   - Use joins/aggregations where needed instead of per-row lookups
   - Do not add migrations unless strictly necessary

Definition of done (Part B): list endpoint does filtering/sorting/pagination in query layer; handler doesn’t load-all-then-filter.

---

### Part C — Models are plain data (remove query calls from models)

8. Find active-record style call sites:
   - Search for `crate::queries::` usage inside `packages/default/src/models/`
   - For each found (start with systems):
     - move persistence into the service layer
     - keep validation in the model (e.g., `validate_name`, pure constructors)

9. Adjust call sites:
   - Update handlers that used `Model::new(...)` (and inserted) to call `services::...::create_...(...)` instead

Definition of done (Part C): models no longer depend on queries for the touched area (systems).

---

### Part D — Web UI bootstrap split

10. Create bootstrap modules:

- Add:
  - `packages/web-ui/src/bootstrap/mod.rs`
  - `packages/web-ui/src/bootstrap/auth.rs`
  - `packages/web-ui/src/bootstrap/assets.rs`

11. Move auth bootstrapping:

- Identify in `packages/web-ui/src/main.rs`:
  - fetch auth context
  - set app state
  - handle auth loading flags
- Move into:
  - `bootstrap::auth::init_auth(app_state: Signal<AppState>, ...)`
- Call it from root component or a top-level `use_effect` (match existing code style).

12. Move assets injection:

- Identify CSS/script injection bits (highlight.js, styles)
- Move into:
  - `bootstrap::assets::inject_assets()`
- Ensure it’s called once during startup.

13. Make `main.rs` thin:

- `main.rs` should:
  - create state
  - call bootstrap helpers
  - mount the app

Definition of done (Part D): `main.rs` is mostly wiring, not logic.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Guardrails

- Keep changes incremental: do systems end-to-end; don’t refactor other endpoints unless required for compilation.
- Avoid renaming public API fields or changing response structures.
- Prefer small pure structs/enums for filter/sort/pagination types; don’t overengineer.
- If the list endpoint needs joins/derived fields, implement those in the query layer, not in handlers.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
<!-- SECTION:FINAL_SUMMARY:BEGIN -->

### Deliverables

- New backend modules:
  - `packages/default/src/services/mod.rs`
  - `packages/default/src/services/systems.rs`
- Updated backend files:
  - `packages/default/src/handlers/api/systems.rs`
  - `packages/default/src/queries/systems.rs` (or appropriate file)
  - `packages/default/src/models/systems.rs` (remove query calls)
- New UI modules:
  - `packages/web-ui/src/bootstrap/mod.rs`
  - `packages/web-ui/src/bootstrap/auth.rs`
  - `packages/web-ui/src/bootstrap/assets.rs`
- Updated UI files:
  - `packages/web-ui/src/main.rs`
- Tests:
  - Backend tests for list filter/sort/pagination and RBAC/env scope behavior

<!--
SECTION:FINAL_SUMMARY:END
-->
<!-- SECTION:FINAL_SUMMARY:END -->
