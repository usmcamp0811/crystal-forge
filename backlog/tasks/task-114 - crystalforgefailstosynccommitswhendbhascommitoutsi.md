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
-->\n\n# Issue Details\n\n- **Issue ID:** 184209540\n- **Issue IID:** 114\n- **Title:** Crystal Forge fails to sync commits when DB has commit outside shallow clone history\n- **State:** opened\n- **Labels:** type::bug\n- **Created by:** Matt\n- **Created at:** 2026-02-15T17:07:23.813Z\n- **Updated at:** 2026-02-15T17:09:51.229Z\n\n# Description\n\nBug Report
Description
Commit sync fails with git log invalid revision range when the last stored commit hash is not present in the shallow clone (e.g., after force‑push or history rewrite). This prevents new commits from being detected and inserted.
Steps to Reproduce
1. Store a commit hash in the DB for a flake that is no longer present in the branch history (force‑push or old history).
2. Run Crystal Forge with since_commit sync enabled (default behavior).
3. Observe commit sync for that flake.
Expected Behavior
Crystal Forge should recover from missing commit history by deepening/unshallowing or re‑initializing from HEAD, and continue syncing new commits.
Actual Behavior
Commit sync fails with:
git log failed: fatal: Invalid revision range <old_commit>..HEAD
No new commits are inserted for that flake.
Environment
- Crystal Forge version: unknown
- Component affected: server (flake commit sync)
- NixOS version: 25.11
- Deployment method: NixOS module
Logs
WARN crystal_forge::flake::commits: Failed to sync new commits for campground: git log failed: fatal: Invalid revision range 861eeae9038a44779fee3cc755443a762e523050..HEAD
Database State (if relevant)
SELECT id, name, repo_url FROM flakes WHERE name = 'campground';
SELECT git_commit_hash FROM commits WHERE flake_id = 1 ORDER BY commit_timestamp DESC LIMIT 1;
Configuration
[[flakes.watched]]
auto_poll = true
initial_commit_depth = 10
name = "campground"
repo_url = "https://git.lan.aicampground.com/campground/config?ref=main"
Additional Context
Manual clone of the repo (git clone --depth 50 --branch main --single-branch …) does not contain the DB’s last commit hash, so git log <hash>..HEAD fails. This happens when the repo history is rewritten or the DB contains an older commit outside the shallow depth.
Workaround
Clear commits for the flake in the DB so Crystal Forge re‑initializes from current HEAD:
DELETE FROM derivations WHERE commit_id IN (SELECT id FROM commits WHERE flake_id = 1);
DELETE FROM commits WHERE flake_id = 1;\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n