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
-->\n\n# Issue Details\n\n- **Issue ID:** 171723633\n- **Issue IID:** 92\n- **Title:** Optimize Derivation Dry-Run Processing Performance\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-08-10T16:59:20.669Z\n- **Updated at:** 2025-08-10T17:31:10.892Z\n\n# Description\n\n## Problem
Large backlog of pending dry-run derivations due to slow Nix evaluation bottleneck. Current issues:
- **Sequential processing** (concurrency limit = 1) 
- **Slow Nix operations**: Each `nix build --dry-run` takes 10-60+ seconds
- Only processing one derivation at a time while others wait
- No parallelization of expensive Nix evaluations

## Impact
- **Performance**: Massive backlog due to slow sequential processing
- **Resource Usage**: CPU/memory underutilized (only 1 Nix process at a time)
- **Scalability**: Processing time grows linearly with derivation count

## Solution
1. **Increase Concurrency**: Process 5-10 derivations simultaneously (main fix)
2. **Parallel Nix Evaluations**: Run multiple `nix build --dry-run` processes concurrently
3. **Smart Batching**: Group derivations by flake to optimize git checkouts
4. **Resource Limits**: Use systemd to prevent resource exhaustion
5. **Minor optimizations**: Cache config, optimize DB queries

## Expected Results
- **5-10x throughput improvement** by parallelizing Nix evaluations
- Better CPU/memory utilization
- Faster backlog clearing with same resources

## Files to Change
- `server/mod.rs` - Rewrite `process_pending_derivations()`
- `queries/derivations.rs` - Add optimized query with JOINs
- Add database indexes for performance\n\n# Assignees\n\nMatt\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n