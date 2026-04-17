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
-->\n\n# Issue Details\n\n- **Issue ID:** 171334773\n- **Issue IID:** 87\n- **Title:** Crystal Forge Builder: OOM Kills and Resource Management\n- **State:** closed\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-07-31T02:09:40.407Z\n- **Updated at:** 2025-08-01T03:21:56.381Z\n\n# Description\n\n## Problem

The Crystal Forge builder is being OOM-killed by systemd due to high memory usage (8–10GB) during Nix builds. Large packages built from source trigger this, and CVE scans sometimes run before builds finish.

---

## Root Causes

* Memory usage exceeds limits
* Large packages (e.g., Firefox, Chromium) built from source
* No checks before building or throttling
* CVE scans race ahead of failed builds

---

## Tasks

**Systemd Fixes**

* [ ] Set `MemoryMax=16G`, `MemoryHigh=12G`
* [ ] Enable 8GB swap
* [ ] Set `CPUQuota=200%` (2 cores)
* [ ] Add working directory and `TMPDIR`
* [ ] Use `Restart=on-failure` with backoff

**Nix Settings**

* [ ] Use `fallback = false` to avoid source builds
* [ ] Enable `substitute = true` and `builders-use-substitutes = true`
* [ ] Configure extra binary caches (e.g., nix-community)
* [ ] Limit to `max-jobs = 1`, `cores = 2`

**Builder Logic**

* [ ] Add dry-run analysis with `nix build --dry-run`
* [ ] Detect heavy packages from derivations
* [ ] Check cache availability with `nix path-info`
* [ ] Skip builds likely to be expensive if no binary cache
* [ ] Use `-j1` for heavy packages
* [ ] Add timeouts and safety checks

**CVE Scan Fixes**

* [ ] Only run scans after successful builds
* [ ] Check that derivation exists before scanning
* [ ] Trigger scans via build completion callbacks

**Packages to Flag as Heavy**

```
firefox, chromium, thunderbird, libreoffice, webkit, qtwebengine,
llvm, gcc, rust, nodejs, electron, blender, gimp, mesa, linux-kernel
```

**Integration**

* [ ] Replace direct `nix build` calls with smart logic
* [ ] Log build analysis and resource estimates
* [ ] Track skipped builds in the database
* [ ] Link CVE scans to completed builds

---\n\n# Milestone\n\n{
  "id": 6040389,
  "iid": 5,
  "group_id": 0,
  "project_id": 70402481,
  "title": "v0.4.0 - Enterprise Security",
  "description": "**Goal**: Prepare for LAN or production deployment.\r\n\r\n* [x] Shared secret or token auth for agents\r\n* [ ] Harden webhook input (validate source repo)\r\n* [ ] Limit derivation evaluation to allowlist\r\n* [ ] Add systemd service definition + journald config\r\n* [ ] Write tests for all HTTP handlers + flake logic",
  "start_date": null,
  "due_date": null,
  "state": "active",
  "web_url": "https://gitlab.com/crystal-forge/crystal-forge/-/milestones/5",
  "updated_at": "2025-06-28T02:47:15.836Z",
  "created_at": "2025-06-14T03:46:26.876Z",
  "expired": false
}\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n