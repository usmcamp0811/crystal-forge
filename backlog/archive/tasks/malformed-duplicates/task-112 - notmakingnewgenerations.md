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
-->\n\n# Issue Details\n\n- **Issue ID:** 179275967\n- **Issue IID:** 112\n- **Title:** Not making new Generations\n- **State:** opened\n- **Labels:** type::bug\n- **Created by:** Matt\n- **Created at:** 2025-12-24T21:08:55.31Z\n- **Updated at:** 2026-01-04T23:59:36.989Z\n\n# Description\n\nSystems are rebooting to previous generations because the current generation doesn't show in the GRUB menu. I suspect there is another script that needs to be run when deploying a system beyond just:

```
/nix/store/gjdlfgjldfgjdgflj-my-system/bin/switch-to-configuration switch
```

Need to nail this down. In a perfect world this is not a real big deal because when the system boots it'll start Crystal Forge which will trigger a redeploy but in the situation that the system is some how isolated from the CF server such as a power / network outage, this could be problematic. This could also be a problem for newly onboarded systems reverting back to a non CF state..\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n