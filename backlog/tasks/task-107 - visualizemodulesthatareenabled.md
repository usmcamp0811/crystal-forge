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
-->\n\n# Issue Details\n\n- **Issue ID:** 177108882\n- **Issue IID:** 107\n- **Title:** Visualize Modules that are Enabled\n- **State:** opened\n- **Labels:** feature::dashboard, feature::flake-tracking, feature::self-service, feature::stig\n- **Created by:** Matt\n- **Created at:** 2025-11-14T14:20:04.312Z\n- **Updated at:** 2025-11-14T14:20:04.312Z\n\n# Description\n\n## Feature Request

### Problem Statement
It would be neat if we could have a way to visualize the NixOS modules that are enabled (with options). 
### Proposed Solution

We could simply during the eval job we could get all the modules for a system. Then maybe store it in Postgres as a BSON or something I don't know. 

### Use Case

This would allow a user to inspect and see what each system actually has enabled at a high enough level that doesn't require looking at Nix source code which could be beneficial to the SysAdmins or be used for making some sort of roll up report about systems. 


### Alternatives Considered

This could relate to STIG work as it would make a nice way to see to what parts of a STIG are implemented on a given system. If we do this we could at a later point in time implement a way to make commits back to the flake repo so as to effect changes to whats running on systems, but this is a super stretch goal and not something that needs to be done right away.



### Impact
<!-- Who would benefit from this feature? -->
- [X] Improves compliance/audit capabilities
- [ ] Reduces operational overhead
- [ ] Enhances security
- [ ] Better developer experience
- [ ] Performance improvement
- [ ] Other: 

### Additional Context

Might be able to look at or do something similar to [SnowflakeOS](https://snowflakeos.org/) and limit support for being able to modify Systems.\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n