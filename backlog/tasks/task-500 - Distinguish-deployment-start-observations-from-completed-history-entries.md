---
id: TASK-500
title: Distinguish deployment-start observations from completed history entries
status: Backlog
assignee: []
created_date: '2026-10-20 05:20'
labels: []
dependencies: []
references:
  - packages/web-ui/src/views/system_detail.rs
  - docs/design/CrystalForge/components/SystemDetail.jsx
  - docs/knowledge/ui/deployment-history-rendering-rules.md
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/335'
priority: medium
type: bug
ordinal: 551000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The connected Delivery browser proof after two completed real deployments displays '4 deploys'. Each actual success card is accompanied by a 'Deployment started' card rendered as 'Deployed', 'success', and 'no generation activated'. Backend completed deployment/state/history correlation is separately verified, so Started must not be used as completed-activation evidence. Screenshot: integration-evidence/integration-delivery/delivery-lifecycle/browser/inspect-second/delivery-second-history-after-reload.png from /nix/store/mrs8rm4yvf5lk9vklqs01qvmzp1zl9n7-vm-test-run-cf-it-delivery-delivery-lifecycle-run. Review approved SystemDetail design and authoritative history contract before changing the labels/count. The existing rollback plan also renders Strategy 'rollback' where the approved component says 'switch_to_generation'; record and resolve that presentation difference under accepted authority. No production UI repair is authorized by the current test increment.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The accepted History presentation distinguishes Started from actual completed activation and defines the displayed deployment count.
- [ ] #2 The rollback plan's strategy label follows the approved presentation or an explicitly recorded product decision.
- [ ] #3 Behavioral browser coverage verifies Started versus completed presentation and durable reload behavior without synthetic completion evidence.
<!-- AC:END -->
