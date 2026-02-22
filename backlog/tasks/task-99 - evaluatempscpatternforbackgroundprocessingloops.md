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
-->\n\n# Issue Details\n\n- **Issue ID:** 173218467\n- **Issue IID:** 99\n- **Title:** Evaluate MPSC Pattern for Background Processing Loops\n- **State:** opened\n- **Labels:** Labels:\n- **Created by:** Matt\n- **Created at:** 2025-09-09T02:18:46.25Z\n- **Updated at:** 2025-09-09T02:18:46.25Z\n\n# Description\n\n## Summary
Evaluate replacing timer-based polling loops with event-driven MPSC (Multi-Producer, Single-Consumer) channels for better responsiveness and resource efficiency.

## Current State
- Git polling loop: Checks repositories every 10 minutes for new commits
- Build processing loop: Polls database every 60 seconds for dry-run-complete derivations  
- CVE scan loop: Polls database every 60 seconds for build-complete derivations
- Cache pushing: Currently synchronous, blocks builds (being converted to MPSC)

## Proposed Changes

### Phase 1: Cache Pushing (In Progress)
- [x] Convert cache pushing to async MPSC queue #98
- [ ] Test performance and reliability

### Phase 2: Evaluation (To Be Assessed)
**Git Polling → Evaluation Processing**
- Replace timer with webhook triggers or filesystem watching
- Use MPSC to queue evaluation jobs when commits detected
- Benefits: Immediate processing vs up to 10-minute delay
- Risks: Webhook infrastructure complexity

**Build Processing Loop**
- Convert database polling to MPSC work queue
- Benefits: Questionable - database is already a queue
- Risks: Added complexity, potential message loss vs persistent DB queue

**CVE Scan Loop**  
- Similar to build processing - evaluate if MPSC adds value over DB polling

## Success Criteria
- Improved responsiveness (lower latency from trigger to processing)
- Reduced resource usage (no unnecessary polling)
- Maintained reliability (no lost work)
- Code complexity remains manageable

## Questions to Answer
1. Do the latency improvements justify the complexity?
2. How do we handle backpressure and failure scenarios?
3. Should we keep database queues for persistence and add MPSC for performance?
4. Are there hybrid approaches that get benefits without full conversion?

## Dependencies
- Complete cache pushing MPSC implementation first
- Measure current polling overhead and latency\n\n# Weight\n\n0\n\n# Time Stats\n- Time Estimate: 0\n- Total Time Spent: 0\n