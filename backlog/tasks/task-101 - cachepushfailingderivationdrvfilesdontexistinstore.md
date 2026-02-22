# Title

<!--
Short, outcome-focused title
-->

---

# Problem

<!--
Brief description of the issue or opportunity.
Keep this lightweight.
-->

---

# Desired Outcome

<!--
What should be true if this is completed?
-->

---

# Notes

<!--
Optional context, links, screenshots, or references.
-->

---

# Scope Hint (Optional)

<!--
If obvious, describe rough boundaries.
Not required at Backlog stage.
-->\n\n# Issue Details\n\n- **Issue ID:** 174117755\n- **Issue IID:** 101\n- **Title:** Cache Push Failing: Derivation .drv Files Don't Exist in Store\n- **State:** closed\n- **Labels:** priority::high, type::bug\n- **Created by:** Matt\n- **Created at:** 2025-09-29T20:58:32.31Z\n- **Updated at:** 2025-09-30T01:59:00.723Z\n\n# Description\n\n### Description
Cache push jobs are failing because the system is trying to resolve `.drv` paths that no longer exist in the Nix store. The error occurs in `cache.rs:resolve_drv_to_store_path()` when running `nix-store --query --outputs` on derivation paths.

**Error from logs:**
```
ERROR crystal_forge::builder: ❌ Cache push failed for derivation ec2-user-authorized_keys: 
nix-store --query --outputs failed: error: path '/nix/store/ziyaya36m8lnws9pic8r5bz507k21a62-ec2-user-authorized_keys.drv' 
does not exist and cannot be created
```

### Root Cause
The `.drv` files are ephemeral and may be garbage collected after builds complete. The current implementation stores the `.drv` path in `derivation_path` and attempts to resolve it to the output path at cache push time, but by then the `.drv` file may no longer exist.

Currently:
- `derivation_path`: Stores the `.drv` file path
- `derivation_target`: Used for nixos-rebuild switch targets (can't be repurposed)
- No field exists to store the actual built output path

### Proposed Solution
Add a new `store_path` field to the `derivations` table to store the actual built output path (which persists in the store).

### Changes Required

1. **Database Migration**
   ```sql
   ALTER TABLE derivations ADD COLUMN store_path TEXT;
   CREATE INDEX idx_derivations_store_path ON derivations(store_path) WHERE store_path IS NOT NULL;
   ```

2. **Update Derivation Model** (`src/models/derivations/mod.rs`)
   - Add `store_path: Option<String>` field

3. **Update Build Completion** (`src/queries/derivations.rs`)
   - Modify `mark_derivation_build_complete()` to accept and store the output `store_path` parameter
   - Update all callers to pass the built output path

4. **Update Cache Push Query** (`src/queries/cache_push.rs`)
   - Change `get_derivations_needing_cache_push()` to check `d.store_path IS NOT NULL` instead of `d.derivation_path IS NOT NULL`
   - Use `store_path` when creating cache push jobs

5. **Update Cache Push Processing** (`src/builder/mod.rs`)
   - Modify `process_cache_pushes()` to use `derivation.store_path` instead of `derivation.derivation_path`

6. **Update All SELECT Queries**
   - Ensure all `sqlx::query_as!` calls that return `Derivation` include `store_path` in SELECT and RETURNING clauses

### Schema After Changes
- `derivation_path`: The `.drv` file path (for reference, may be GC'd)
- `store_path`: The actual built output path (persists, used for cache push)
- `derivation_target`: The nixos-rebuild target (unchanged)

### Related Files
- `src/models/derivations/mod.rs`
- `src/models/derivations/build.rs`
- `src/queries/derivations.rs`
- `src/queries/cache_push.rs`
- `src/builder/mod.rs`\n\n# Assignees\n\nMatt\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n