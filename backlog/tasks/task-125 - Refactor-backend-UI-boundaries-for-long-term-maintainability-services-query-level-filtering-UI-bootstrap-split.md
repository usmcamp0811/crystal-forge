---
id: TASK-125
title: >-
  Refactor backend/UI boundaries for long-term maintainability (services +
  query-level filtering + UI bootstrap split)
status: To Do
assignee: []
created_date: '2026-02-24 13:46'
labels: []
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
**Goal**
Reduce handler/UI hotspot bloat and clarify architectural boundaries by introducing a lightweight service layer, moving list filtering/pagination into query functions, and splitting Web UI bootstrap concerns out of `main.rs`. Keep behavior identical.

**Why (problems this solves)**

* API handlers are mixing auth/RBAC + orchestration + filtering + DTO mapping, making changes risky.
* Some “models” perform persistence (calling queries), blurring domain vs storage boundaries.
* Systems listing work is done in-memory, which won’t scale and encourages duplicate logic.
* `web-ui/src/main.rs` is accumulating unrelated concerns (auth fetch, asset injection, bootstrapping).

**Non-goals**

* No big rewrite or new framework.
* No behavior/UI changes.
* No new auth model or RBAC redesign.
* No database schema changes (unless strictly required to support server-side pagination/filtering—and if so, keep minimal and justified).

---

## Scope

This task covers:

### Backend (`packages/default`)

1. Add a **service layer** and move orchestration/policy out of handlers:

   * `src/services/mod.rs`
   * `src/services/systems.rs`
   * (optional if needed) `src/services/deployments.rs` or similar for adjacent operations
2. Move **systems list filtering/sorting/pagination** into query functions:

   * update `src/queries/systems.rs` (or relevant file)
3. Clarify **model responsibilities**:

   * remove direct calls from `models/*` into `queries/*` where encountered (start with systems).
   * Replace with service functions called from handlers.

### Web UI (`packages/web-ui`)

4. Split `main.rs` into explicit “bootstrap” modules:

   * `src/bootstrap/mod.rs`
   * `src/bootstrap/auth.rs` (auth fetch / hydration)
   * `src/bootstrap/assets.rs` (CSS/script injection)
   * Keep `main.rs` thin: launch + root component composition.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `handlers/api/systems.rs` no longer performs in-memory filtering/sorting/pagination; it delegates to `services::systems::list_systems_for_user(...)` (or similar).
- [ ] #2 Query layer (`queries/systems.rs`) supports server-side filters + sorting + pagination that match existing behavior.
- [ ] #3 No `models/*` module calls into `queries/*` directly for creation/insertion for the refactored domain (at least systems). Models are plain data + validation helpers only.
- [ ] #4 All existing endpoints continue to return the same JSON schema and semantics.
- [ ] #5 Unit tests added for query filter/sort/pagination logic (can be DB-backed integration tests if that’s what you already use).
- [ ] #6 At least one test covering “RBAC scoping + env membership filtering” path for systems list (existing tests may be extended).
- [ ] #7 `web-ui/src/main.rs` contains no auth-fetch logic and no raw asset injection logic.
- [ ] #8 `nix flake check` (or your repo’s check entrypoints) passes.
- [ ] #9 `cargo test` passes for both crates.
- [ ] #10 Filter by environment (member vs non-member)
- [ ] #11 Sort order stable and correct
- [ ] #12 Pagination returns correct window and correct `total_count`
* If you already have DB test utilities/fixtures, use them.
* If not, write “query unit tests” with a temporary test DB (whatever repo pattern exists).
- [ ] #13 Build the web UI.
- [ ] #14 `cargo fmt` clean, `clippy` clean (or matches current repo policy).
- [ ] #15 Auth bootstrap is isolated in `bootstrap/auth.rs` and invoked from the root component or startup path.
- [ ] #16 Asset/script/style injection is isolated in `bootstrap/assets.rs`.
- [ ] #17 UI behavior is identical (including fallback/mock behavior if already implemented elsewhere).
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
### Part A — Backend service layer (systems)

1. **Create services module**

   * Add `packages/default/src/services/mod.rs`
   * Export submodules:

     * `pub mod systems;`

2. **Create systems service**

   * Add `packages/default/src/services/systems.rs`
   * Define service functions that represent use-cases, not SQL:

     * `list_systems_for_user(...)`
     * (optional) `get_system_details(...)`
     * (optional) `create_system(...)` if you touch creation flows
   * Inputs should be:

     * auth context / user id
     * user role(s) or permission context
     * filter/sort/pagination options
   * Output should be:

     * API-ready DTOs or (preferably) domain structs + mapping in handler. Pick one and be consistent.

3. **Move orchestration out of handler**

   * In `handlers/api/systems.rs`, identify blocks doing:

     * auth extraction
     * membership/environment lookup
     * filtering/sorting/pagination
     * assembling response objects
   * Replace these with a call into `services::systems::list_systems_for_user`.
   * Keep the handler responsible only for:

     * extracting request params
     * pulling auth/session info
     * calling service
     * returning HTTP response

**Definition of done for Part A:** handler is “skinny” and reads like a controller.

---

### Part B — Query-level filtering/sorting/pagination

4. **Introduce explicit filter/sort/pagination types**

   * In `services/systems.rs` or `queries/systems.rs`, define:

     * `SystemsListFilter` (e.g. env_id, name contains, status, etc. based on existing behavior)
     * `SystemsSort` (field + direction)
     * `Pagination` (limit + offset OR page + page_size)
   * Keep it small: only what current UI/handler already supports.

5. **Implement query function**

   * In `queries/systems.rs`, implement something like:

     * `list_systems_scoped(pool, scope, filter, sort, pagination) -> (Vec<SystemRow>, total_count)`
   * “scope” should represent RBAC/env membership rules:

     * “admin sees all”
     * “operator/viewer sees only environments they’re in”
     * match current behavior exactly

6. **Remove in-memory filtering**

   * Ensure the service requests systems using the new query.
   * Delete old in-memory filter/sort/pagination code from handler.

7. **Performance sanity check**

   * Ensure query uses indexes where obvious (don’t add migrations unless required).
   * Avoid N+1 queries in the list endpoint; if the existing code did per-row lookups, consolidate.

**Definition of done for Part B:** list endpoint does filtering/sorting/pagination in SQL/query layer; handler doesn’t “load everything then filter.”

---

### Part C — Models are plain data (remove query calls from models)

8. **Find active-record style call sites**

   * Search for `crate::queries::` usage inside `src/models/`.
   * For each found (start with systems):

     * move persistence into a service function
     * keep validation in the model (e.g. `validate_name`, constructors that do not hit DB)

9. **Adjust call sites**

   * Update handlers that used `Model::new(...)` (that also inserted) to call `service::create_...(...)` instead.

**Definition of done for Part C:** models no longer depend on queries for the touched area (systems).

---

### Part D — Web UI bootstrap split

10. **Create bootstrap modules**

* Add:

  * `web-ui/src/bootstrap/mod.rs`
  * `web-ui/src/bootstrap/auth.rs`
  * `web-ui/src/bootstrap/assets.rs`

11. **Move auth bootstrapping**

* Identify in `web-ui/src/main.rs`:

  * “fetch auth context”
  * “set app state”
  * “handle auth loading flags”
* Move those into a function:

  * `bootstrap::auth::init_auth(app_state: Signal<AppState>, ...)`
* Keep it called from the root component or a top-level `use_effect` (whatever is idiomatic in your codebase).

12. **Move assets injection**

* Identify CSS/script injection bits (highlight.js, styles).
* Move into:

  * `bootstrap::assets::inject_assets()`
* Call it once near startup.

13. **Make `main.rs` thin**

* `main.rs` should:

  * create state
  * call bootstrap helpers
  * mount the app

**Definition of done for Part D:** main.rs is mostly wiring, not logic.

---
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Notes / Guardrails

* Keep changes incremental: do **systems** first end-to-end; don’t refactor other endpoints unless required for compilation.
* Avoid renaming public API fields or changing response structures.
* Prefer small pure structs/enums for filter/sort/pagination types—don’t overengineer.
* If you discover the list endpoint currently relies on extra joins/derived fields, implement those in the query layer, not in handlers.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Deliverables

* New backend modules:

  * `packages/default/src/services/mod.rs`
  * `packages/default/src/services/systems.rs`
* Updated backend files:

  * `packages/default/src/handlers/api/systems.rs`
  * `packages/default/src/queries/systems.rs` (or appropriate file)
  * `packages/default/src/models/systems.rs` (remove query calls)
* New UI modules:

  * `packages/web-ui/src/bootstrap/mod.rs`
  * `packages/web-ui/src/bootstrap/auth.rs`
  * `packages/web-ui/src/bootstrap/assets.rs`
* Updated UI files:

  * `packages/web-ui/src/main.rs`
* Tests:

  * backend tests for list behavior (and any updated existing tests)
<!-- SECTION:FINAL_SUMMARY:END -->
