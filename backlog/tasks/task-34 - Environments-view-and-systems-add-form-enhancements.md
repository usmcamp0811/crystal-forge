---
id: TASK-34
title: Environments view and systems add-form enhancements
status: Done
assignee:
  - KimiK2.5
created_date: '2026-02-17 03:00'
updated_date: '2026-02-21 03:28'
labels:
  - ui
  - web-ui
  - systems
  - environments
milestone: m-9
dependencies: []
priority: high
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add Environments page with add/remove actions; update systems add form to use environment dropdown and keypair generation modal that auto-fills public key.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Added Environments route/view with local registry add/remove UX and safe remove guard; updated Systems add form to required fields (hostname/public key/environment/flake/policy), environment dropdown from existing environments, and keypair generation modal that inserts generated public key.

Implemented client-side Ed25519 keypair generation modal (Web Crypto randomness + ed25519-dalek), with private/public output and one-click public-key injection into add-system form.

Refined environments UX: required-policy chips/counts, enforcement-pending badge, add-environment default required agent policy, and edit-requirements modal with policy-library link.
<!-- SECTION:NOTES:END -->
