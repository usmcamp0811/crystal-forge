# Crystal Forge fails to sync commits when DB has commit outside shallow clone history

---

# Problem
Commit sync fails with git log invalid revision range when the last stored commit hash is not present in the shallow clone (e.g., after force‑push or history rewrite). This prevents new commits from being detected and inserted.

---

# Desired Outcome
Crystal Forge should recover from missing commit history by deepening/unshallowing or re‑initializing from HEAD, and continue syncing new commits.

---

# Notes
- Labels: type::bug
- Created: about 5 days ago
- Component affected: server (flake commit sync)
- NixOS version: 25.11
- Deployment method: NixOS module
- Logs show: WARN crystal_forge::flake::commits: Failed to sync new commits for campground: git log failed: fatal: Invalid revision range 861eeae9038a44779fee3cc755443a762e523050..HEAD
- Environment: Manual clone of the repo (git clone --depth 50 --branch main --single-branch …) does not contain the DB's last commit hash, so git log <hash>..HEAD fails. This happens when the repo history is rewritten or the DB contains an older commit outside the shallow depth.
- Workaround: Clear commits for the flake in the DB so Crystal Forge re‑initializes

---

# Scope Hint (Optional)
Fix the commit sync logic to handle missing commit hashes in shallow clones, possibly by implementing automatic deepening or fallback to HEAD initialization.