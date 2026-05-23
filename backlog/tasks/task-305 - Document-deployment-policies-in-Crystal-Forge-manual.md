---
id: TASK-305
title: Implement deployment policy system for fleet management
status: Backlog
assignee: []
created_date: '2026-05-23 14:12'
updated_date: '2026-05-23 14:17'
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
Design and implement advanced deployment policies for Crystal Forge's existing policy system. The basic policies (manual, auto_latest, pinned) already exist. This task adds sophisticated policies for CVE gating, time-based restrictions, multi-approver workflows, and canary deployments.

## Current State

Crystal Forge has a deployment policy system with 3 built-in policies:
1. **manual** - Operator explicitly approves every deploy
2. **auto_latest** - Auto-deploy newest passing commit
3. **pinned** - Stay on specific commit until manually changed

## Goals

Extend the policy system to support:
- **CVE-based gating** - Block deployments introducing vulnerabilities
- **Time-window restrictions** - Limit deployments to specific times/days
- **Multi-approver workflows** - Require N approvers with specific roles
- **Canary/phased rollouts** - Deploy to subsets of fleet, observe, then continue

## Policies to Implement

1. **cve-gated** - Block deploys introducing CVEs
   - Rule: max 0 critical CVEs
   - Rule: max 2 high CVEs  
   - Rule: evaluation must pass
   - Rule: build must succeed (and be cached)
   - Requires: CVE scanning integration (Nix vulnerability scanner or other)

2. **business-hours** - Time-windowed auto-deploy
   - Rule: deploy window (e.g., mon-fri 09:00-17:00 America/New_York)
   - Rule: evaluation must pass
   - Rule: build must succeed (and be cached)
   - Requires: Time zone handling, schedule evaluation

3. **two-approver** - Multi-operator approval
   - Rule: 2 approvers required with admin role
   - Rule: evaluation must pass
   - Rule: build must succeed (and be cached)
   - Requires: Approval tracking, role verification

4. **canary-25** - Phased rollout with observation
   - Rule: canary 25% at a time, observe 30min between phases
   - Rule: evaluation must pass
   - Rule: build must succeed (and be cached)
   - Requires: Fleet grouping/selection, rollout state tracking, time-based progression

## Architecture Considerations

- **CVE scanning**: How to integrate vulnerability data? Nix built-in scanner? External service?
- **Fleet state tracking**: Does CF currently track which systems run which commits?
- **Canary selection**: How to select 25% of systems? Random? By label/group? Deterministic?
- **Approval persistence**: Where to store approval records? Database? Event log?
- **Time-based triggers**: Background job to re-evaluate policies during allowed windows?
- **Rollout orchestration**: How to pause between canary phases and monitor for issues?

## Deliverables

- Design document or RFC for advanced policy architecture
- Implementation of all 4 advanced policies
- CVE scanning integration (if not already present)
- Approval workflow UI/API for multi-approver
- Fleet grouping/selection mechanism for canary
- Time-based policy evaluation (scheduler or cron-like)
- Policy rule extension framework (if needed for new rule types)
- Documentation updates:
  - How each advanced policy works
  - Configuration examples for each policy
  - CVE threshold tuning guidance
  - Time window configuration syntax
  - Approver role requirements
  - Canary deployment behavior and rollback

## Non-Goals

- Changing existing MVP policies (manual, auto_latest, pinned)
- Full observability dashboard (separate task)
- Automated rollback on canary failures (could be follow-up task)
- Policy composition/inheritance (keep policies independent for now)
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CVE-gated policy blocks deployments exceeding vulnerability thresholds
- [ ] #2 CVE-gated policy integrates with vulnerability scanning (Nix or other)
- [ ] #3 Business-hours policy only allows deployments within configured time windows
- [ ] #4 Business-hours policy correctly handles time zones
- [ ] #5 Two-approver policy requires 2 distinct approvals from admin-role operators
- [ ] #6 Two-approver policy tracks and persists approval state
- [ ] #7 Canary-25 policy deploys to 25% of systems initially
- [ ] #8 Canary-25 policy observes for configured duration before next phase
- [ ] #9 Canary-25 policy can select system subsets deterministically
- [ ] #10 All 4 policies integrate with existing policy assignment mechanism
- [ ] #11 All 4 policies respect evaluation and build success rules
- [ ] #12 Documentation explains CVE threshold configuration
- [ ] #13 Documentation explains time window syntax and examples
- [ ] #14 Documentation explains approver workflow
- [ ] #15 Documentation explains canary rollout behavior
- [ ] #16 Tests verify CVE blocking logic
- [ ] #17 Tests verify time window enforcement
- [ ] #18 Tests verify approval counting and role checks
- [ ] #19 Tests verify canary phase progression
<!-- AC:END -->
