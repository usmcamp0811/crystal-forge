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
-->\n\n# Issue Details\n\n- **Issue ID:** 171722466\n- **Issue IID:** 90\n- **Title:** Remove hardcoded 4GB ulimit that overrides systemd memory limits\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-08-10T14:20:57.787Z\n- **Updated at:** 2025-08-10T14:27:14.075Z\n\n# Description\n\n**Description:**

The `builderScript` contains a hardcoded `ulimit -v $((4 * 1024 * 1024))` that limits virtual memory to 4GB, overriding the configured systemd memory limits (64GB).

This causes builds to fail with "out of memory" even when systemd scoping is properly configured.

**Fix:** Remove the ulimit line or make it conditional on `!use_systemd_scope`.

**Evidence:** Logs show "Heap size: 3299 MiB" hitting the 4GB ulimit, not the 64GB systemd limit.\n\n# Milestone\n\n{
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