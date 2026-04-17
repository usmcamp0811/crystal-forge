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
-->\n\n# Issue Details\n\n- **Issue ID:** 173102985\n- **Issue IID:** 94\n- **Title:** Add Expiration and Re-Scan Scheduling for CVE Scans\n- **State:** opened\n- **Labels:** component::monitoring, security::audit, security::vulnerability, type::security\n- **Created by:** Matt\n- **Created at:** 2025-09-06T14:18:49.527Z\n- **Updated at:** 2025-09-06T14:18:49.527Z\n\n# Description\n\n**Description:**
Currently, CVE scans are tied to specific derivations and may not be repeated once completed. This can lead to outdated results if vulnerabilities are published after the initial scan.

We should introduce an **expiration date for CVE scan results** so that scans are rerun on a schedule, regardless of whether a derivation has been rebuilt.

**Proposed Behavior:**

* Each CVE scan result has an expiration (e.g., 7 days).
* If a derivation/system has been scanned before but the results have expired, the system should be rescanned.
* Continuous scanning should focus on **currently deployed configs/systems**, not just newly built derivations.

**Example Scenario:**

* 5 systems deployed across various commits for a week or more.
* Even if scans were already performed at deployment time, the system should be rescanned **at least once per week** to detect new vulnerabilities.

**Benefits:**

* Ensures up-to-date vulnerability visibility.
* Avoids unnecessary rebuilds and rescans of unchanged derivations.
* Provides continuous security monitoring for deployed systems.

**Next Steps / Tasks:**

* [ ] Add expiration metadata to CVE scan results.
* [ ] Implement scheduled re-scan logic (weekly by default).
* [ ] Ensure scans cover all actively deployed systems.
* [ ] Document behavior and configuration options (e.g., expiration interval).\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n