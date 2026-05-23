---
id: TASK-305
title: Document deployment policies in Crystal Forge manual
status: Backlog
assignee: []
created_date: '2026-05-23 14:12'
labels:
  - documentation
  - deployment
  - policies
  - manual
dependencies: []
priority: medium
ordinal: 252000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add comprehensive documentation for Crystal Forge deployment policies to the user manual. This should cover all built-in and example policies including manual approval, auto-deploy strategies, pinning, CVE gating, time-based restrictions, multi-approver workflows, and canary deployments.

The documentation should explain:
- How deployment policies work in Crystal Forge
- The policy rule system and composition
- Each built-in policy type with examples
- How to create and customize policies
- How policies are assigned to systems
- Policy evaluation flow and approval processes

Built-in policies to document:
1. **manual** - Operator must explicitly approve every deploy (built-in, no rules)
2. **auto_latest** - Auto-deploy newest passing commit on assigned flake/branch (built-in, evaluation + build rules)
3. **pinned** - Stay on specific commit until manually changed (built-in, pinned commit rule)

Example/custom policies to document:
4. **cve-gated** - Block deploys with critical CVEs (max 0 critical, max 2 high, evaluation + build)
5. **business-hours** - Auto-deploy only 09:00-17:00 weekdays US-East (time window + evaluation + build)
6. **two-approver** - Requires 2 admin approvers (2 approvers with admin role + evaluation + build)
7. **canary-25** - Roll out to 25% of systems, observe 30min, continue (canary rule + evaluation + build)

The documentation should be user-friendly, include practical examples, and help operators understand when to use each policy type.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Documentation covers all 3 built-in policies (manual, auto_latest, pinned)
- [ ] #2 Documentation includes all 4 example custom policies (cve-gated, business-hours, two-approver, canary-25)
- [ ] #3 Each policy entry explains its purpose, rules, and use cases
- [ ] #4 Documentation explains the policy rule system and how rules compose
- [ ] #5 Documentation includes examples of assigning policies to systems
- [ ] #6 Documentation explains policy evaluation flow and approval processes
- [ ] #7 Content is integrated into the Crystal Forge manual structure
- [ ] #8 Examples are clear and actionable for operators
<!-- AC:END -->
