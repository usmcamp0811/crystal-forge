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
-->\n\n# Issue Details\n\n- **Issue ID:** 171723034\n- **Issue IID:** 91\n- **Title:** Fix Database Connection Pool Exhaustion in Derivation Processing\n- **State:** closed\n- **Labels:** type::bug\n- **Created by:** Matt\n- **Created at:** 2025-08-10T15:42:46.112Z\n- **Updated at:** 2025-08-10T16:15:42.454Z\n\n# Description\n\n## Problem
Derivation processing is failing with "connecting to database" errors due to database connection pool exhaustion. Each concurrent derivation evaluation creates a new database pool instead of reusing the existing connection.

## Symptoms
```
ERROR crystal_forge::queries::derivations: ❌ Failed to process derivation: connecting to database
INFO crystal_forge::server: 💥 Marked target chesty as failed
INFO crystal_forge::server: 💥 Marked target ermy as failed
INFO crystal_forge::server: 💥 Marked target mattis as failed
```

## Root Cause
In `server/mod.rs::process_pending_derivations()`, concurrent tasks call `target.evaluate_and_build()` which internally creates new database pools via `CrystalForgeConfig::db_pool().await?`, causing connection exhaustion.

## Solution
1. Modify existing `Derivation::evaluate_and_build()` method to accept `pool: &PgPool` parameter
2. Update all call sites to pass the existing pool reference instead of creating new pools
3. Remove internal `CrystalForgeConfig::db_pool().await?` calls from the method

## Files to Change
- `models/derivations.rs` - Add new method with pool parameter
- `server/mod.rs` - Update concurrent processing to use pool-aware method\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n