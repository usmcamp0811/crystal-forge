---
id: TASK-183
title: Implement environment-scoped multi-tenant access control across API and UI
status: Backlog
assignee: []
created_date: '2026-03-11 13:25'
labels:
  - security
  - multi-tenant
  - rbac
  - environments
  - api
  - ui
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem:
Crystal Forge currently exposes cache/system/config metadata broadly enough that users from one organization could see resources, routing details, or secrets-related metadata from another organization.

Desired outcome:
Support true multi-tenant operation in a single Crystal Forge deployment by making environment membership the primary data-visibility boundary. A user should only be able to view systems, caches, secrets-related fields, jobs, and environment-linked resources for environments they are explicitly allowed to access.

Scope:
- Tie user access checks consistently to environment membership for read/write endpoints.
- Enforce server-side authorization filters (not UI-only filtering).
- Ensure sensitive fields remain redacted and inaccessible across tenant boundaries.
- Apply the same model to API handlers and UI data fetches so cross-tenant enumeration is not possible.

Non-goals:
- Full org management UX redesign.
- Cross-tenant shared-resource model beyond explicitly global/admin-defined behavior.

Risk:
High (security and tenant isolation).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 For non-admin users, all environment-scoped list/get API endpoints return only resources linked to environments the user is authorized for.
- [ ] #2 Direct object access (by ID/name) to a resource outside the user’s allowed environments is denied (404 or 403 per API contract) with no sensitive metadata leakage.
- [ ] #3 Cache destination and cache job visibility is restricted by environment access; users cannot enumerate caches for unauthorized environments.
- [ ] #4 Secret-bearing fields are redacted in all API responses and are never exposed to users lacking access to the linked environment(s).
- [ ] #5 UI views (systems, caches, related details) show only resources returned by authorized API scope and do not expose cross-tenant data via route navigation.
- [ ] #6 Admin users retain expected global visibility/management behavior (or explicitly documented exceptions).
- [ ] #7 Automated tests cover positive and negative authorization cases for at least systems and caches, including cross-tenant access attempts.
<!-- AC:END -->
