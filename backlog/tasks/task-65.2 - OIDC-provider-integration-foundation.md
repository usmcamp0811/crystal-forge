---
id: TASK-65.2
title: OIDC provider integration foundation
status: Backlog
assignee: ["Claude Opus 4.5"]
labels:
  - security
  - auth
  - oidc
  - backend
milestone: m-14
dependencies:
  - TASK-65.1
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Problem
OIDC provider integration primitives (discovery, key validation, issuer and client configuration) are missing.

Goal
Implement provider-agnostic OIDC integration compatible with Authentik, Keycloak, Entra, Okta, and generic OIDC providers.

Non-Goals
- Provider-specific custom UX.
- Multi-provider runtime tenancy in v1.

Architectural Constraints
- Use standards-compliant OIDC paths.
- Keep provider-specific branching minimal and isolated.
- Security-sensitive validation is in backend or domain, not UI.

Verification Plan
- `nix develop -c cargo test --package default auth::oidc`
- `nix develop -c cargo test --package default auth::jwt_validation`
- `nix develop -c cargo clippy -- -D warnings`
- Manual: validate login handshake against at least one self-hosted provider and one cloud provider.

Impact Areas
- API, Domain, Infrastructure, Security

Risk Level
- High
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 OIDC discovery and JWKS validation is implemented
- [ ] #2 Provider config supports single active provider per deployment
- [ ] #3 Claim extraction supports configurable claim sources
- [ ] #4 Compatibility checklist includes Authentik, Keycloak, Entra, Okta, and generic OIDC
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Follow-up possibility: provider-specific UX tuning if needed.
<!-- SECTION:NOTES:END -->
