---
id: TASK-305
title: Implement deployment policy system for fleet management
status: Backlog
assignee: []
created_date: '2026-05-23 14:12'
updated_date: '2026-05-23 14:15'
labels:
  - feature
  - deployment
  - policies
  - fleet-management
  - architecture
dependencies: []
priority: medium
ordinal: 252000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Design and implement a deployment policy system for Crystal Forge that controls how and when systems are deployed across the fleet. Currently, CF evaluates NixOS sources but lacks formal policy enforcement for deployment decisions.

## Goals

Build a policy engine that enables:
- **Rule-based deployment gating** - Block/allow deployments based on configurable rules
- **Fleet-aware orchestration** - Canary deployments, phased rollouts, subset targeting
- **Multiple policy types** - Manual approval, auto-deploy, pinning, CVE gates, time windows, multi-approver
- **Policy assignment** - Attach policies to systems or groups of systems

## Policy Types to Support

**Built-in policies (MVP):**
1. **manual** - Operator must explicitly approve every deploy (no automatic rules)
2. **auto_latest** - Auto-deploy newest passing commit on assigned flake/branch
3. **pinned** - Stay on specific commit until manually changed

**Extended policies (post-MVP or stretch):**
4. **cve-gated** - Block deploys introducing critical CVEs (max 0 critical, max 2 high)
5. **business-hours** - Auto-deploy only during time windows (e.g., 09:00-17:00 weekdays)
6. **two-approver** - Require N approvers with specific roles
7. **canary-25** - Roll out to X% of systems, observe for duration, continue or halt

## Architecture Questions to Resolve

- Where should policies be stored? (database, Nix config, policy files)
- When/where are policies evaluated? (pre-queue, during orchestration, per-system)
- How does fleet state tracking work? (which systems on which commits)
- How are systems grouped/labeled for canary selection?
- What's the policy definition format? (custom DSL, structured data, code)

## Deliverables

- Policy engine design and implementation
- At minimum: 3 built-in policies (manual, auto_latest, pinned)
- Policy assignment mechanism (UI and/or API)
- Policy evaluation integration into deployment workflow
- Documentation covering:
  - How the policy system works
  - How to assign policies to systems
  - Policy rule reference for each type
  - Examples of creating custom policies (if extensible)
  - Policy evaluation flow

## Non-Goals

- Full observability/audit trail (separate task if needed)
- Advanced policy composition/inheritance (keep simple for MVP)
- External policy engine integration (OPA/etc) - unless clearly beneficial
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Policy data model is defined (schema/types for policies and rules)
- [ ] #2 At least 3 built-in policies are implemented: manual, auto_latest, pinned
- [ ] #3 Policies can be assigned to systems (UI or API)
- [ ] #4 Policy evaluation is integrated into deployment workflow
- [ ] #5 Policy rules are checked before allowing deployments
- [ ] #6 Manual approval policy requires explicit operator action
- [ ] #7 Auto_latest policy automatically deploys passing commits
- [ ] #8 Pinned policy prevents automatic updates until unpinned
- [ ] #9 Documentation explains policy system architecture
- [ ] #10 Documentation includes policy rule reference
- [ ] #11 Documentation shows how to assign policies to systems
- [ ] #12 Tests verify policy evaluation logic
- [ ] #13 Tests verify each built-in policy behavior
<!-- AC:END -->
