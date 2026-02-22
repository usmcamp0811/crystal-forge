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
-->\n\n# Issue Details\n\n- **Issue ID:** 171715201\n- **Issue IID:** 89\n- **Title:** Fix OOM Killer Terminating Crystal Forge Server During Nix Evaluations\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-08-09T18:59:46.975Z\n- **Updated at:** 2025-08-09T21:21:10.448Z\n\n# Description\n\n## Problem Description

The Crystal Forge server process is being killed by the OOM (Out of Memory) killer during Nix flake evaluations, particularly when evaluating large flakes like the dotfiles repository. This causes the entire server to crash and requires manual restart.

### Symptoms
- Server exits with `killed` message during evaluation
- Process terminates consistently at the same evaluation step
- Even simple `nix eval` commands get killed when run as the `crystal-forge` user
- Logs show: `🔍 Evaluating derivation paths for: git+https://gitlab.com/usmcamp0811/dotfiles?rev=...` followed by immediate termination

### Root Cause
Nix evaluations of complex flakes can consume significant memory (>2GB). When the Crystal Forge server runs under systemd with memory limits, the entire cgroup gets OOM-killed when Nix evaluation exceeds available memory.

## Current Behavior
```
INFO crystal_forge::models::derivations: 🔍 Evaluating derivation paths for: git+https://gitlab.com/usmcamp0811/dotfiles?rev=2be96d65bb838f0a9cd73f226831aac2813803bd#nixosConfigurations.chesty.config.system.build.toplevel
[3]    1665784 killed     sudo -u crystal-forge ...
```

## Proposed Solution

Implement **process isolation** for Nix evaluations using `systemd-run` to execute builds in separate resource-controlled scopes, preventing OOM issues from affecting the main server process.\n\n# Assignees\n\nMatt\n\n# Milestone\n\n{
  "id": 6040388,
  "iid": 4,
  "group_id": 0,
  "project_id": 70402481,
  "title": "v0.2.0 - Visibility",
  "description": "**Goal**: Gain insight into system and derivation state.\r\n\r\n* [ ] Web UI or JSON endpoint for known systems\r\n* [ ] Gauge widget: known agents vs. known derivations\r\n* [ ] Table of \"rogue\" or unmatched systems\r\n* [ ] Timeline/history for each system’s state reports\r\n* [ ] Flake status overview (latest commits processed)\r\n",
  "start_date": "2025-06-14",
  "due_date": "2025-07-31",
  "state": "active",
  "web_url": "https://gitlab.com/crystal-forge/crystal-forge/-/milestones/4",
  "updated_at": "2025-06-28T02:45:19.86Z",
  "created_at": "2025-06-14T03:45:35.469Z",
  "expired": true
}\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n