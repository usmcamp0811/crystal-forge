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
-->\n\n# Issue Details\n\n- **Issue ID:** 172679850\n- **Issue IID:** 93\n- **Title:** Test Needed: Inconsistent derivation paths stored when no rebuilds needed\n- **State:** opened\n- **Labels:** status::needs-testing, type::bug\n- **Created by:** Matt\n- **Created at:** 2025-08-30T23:11:30.481Z\n- **Updated at:** 2025-09-07T02:42:56.253Z\n\n# Description\n\n## Problem

When `evaluate_derivation_path()` runs and finds no new derivations need building (everything cached), it stores the **result path** instead of the **derivation path** in the database:

- Database shows: `/nix/store/habccs5jwqx6vj9qb13625mr6vl7x6my-nixos-system-reckless-...drv`  
- Actual system: `/nix/store/64fi8hhlnnk01i7c88ch9gkz4w49qcxl-nixos-system-reckless-...` (no .drv)

This happens in the "no-derivations" error handling path where we fall back to `run_print_out_paths()`, which returns the store result path, not the derivation path.

## Impact

- Database queries expecting .drv paths fail to match actual deployments
- Inconsistent data between fresh builds vs cached builds  
- Makes it impossible to reliably track which derivation produced which result

## Location

`src/models/derivations.rs` in `evaluate_derivation_path()` method, specifically the `Err(e) if e.to_string() == "no-derivations"` handling block.

## Request

Create integration test that:
1. Builds a system configuration twice  
2. Verifies both runs store the same derivation path format
3. Ensures derivation_path column always contains .drv paths
4. Validates that result paths can be derived from derivation paths consistently\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n