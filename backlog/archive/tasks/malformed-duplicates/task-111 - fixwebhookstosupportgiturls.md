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
-->\n\n# Issue Details\n\n- **Issue ID:** 179099640\n- **Issue IID:** 111\n- **Title:** Fix Webhooks to support *.git urls.\n- **State:** opened\n- **Labels:** feature::flake-tracking\n- **Created by:** Matt\n- **Created at:** 2025-12-21T03:48:04.121Z\n- **Updated at:** 2025-12-21T03:48:04.121Z\n\n# Description\n\nIt seems like webhooks can not handle flake urls that look like:

`https://gitlab.com/username/reponame` as when the webhook `GET` request hits Crystal Forge it sees `https://gitlab.com/username/reponame.git` and so the lookup can't find the flake in the database. 

The current work around is to do `https://gitlab.com/username/reponame?ref=branch`. This is good and robust to handle flakes with different branches and what not but its not intuitive. 

We should handle the `https://gitlab.com/username/reponame` case by a fuzy match or just truncate the `.git`.\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n