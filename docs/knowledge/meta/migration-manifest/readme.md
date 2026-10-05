---
type: Reference
title: "Migration manifest: repository README"
description: Maps the repository README.md sections to their OKF destinations and records that README.md was replaced by a short entry point.
tags:
  - crystal-forge
  - migration
---

# Migration manifest: repository README

| Original | Destination | Action | Coverage |
| --- | --- | --- | --- |
| `README.md` | [overview/release-notes-v0-3-0.md](../../overview/release-notes-v0-3-0.md), [overview/project-introduction-and-key-features.md](../../overview/project-introduction-and-key-features.md), [overview/release-roadmap-and-milestones.md](../../overview/release-roadmap-and-milestones.md), [architecture/ecosystem-architecture-summary.md](../../architecture/ecosystem-architecture-summary.md), [operations/authentication-modes-and-oidc-configuration.md](../../operations/authentication-modes-and-oidc-configuration.md), [operations/nixos-module-configuration.md](../../operations/nixos-module-configuration.md), [operations/development-environment-commands.md](../../operations/development-environment-commands.md); `README.md` stays at its path as a short entry point | replaced | complete |

## Source inventory

### `README.md`

- Title: Crystal Forge (logo and tagline header; there is no H1 heading)
- Purpose: Repository entry point that also held release notes, features, architecture, configuration examples, development commands, and the roadmap.
- Action: replaced. The original path now holds a short entry point (logo, description, dashboard screenshot, feature list, documentation links, quick start, contributing, license). The stale logo path `../docs/cf-logo-transparent.png` became `./docs/cf-logo-transparent.png`.
- Sections:
  | Source section | Destination |
  | --- | --- |
  | Header (logo and tagline) | `README.md` (logo path fixed; original line recorded in [overview/project-introduction-and-key-features.md](../../overview/project-introduction-and-key-features.md)) |
  | `## What's New in v0.3.0` (intro, dashboard image) | [overview/release-notes-v0-3-0.md](../../overview/release-notes-v0-3-0.md) |
  | `### Web UI Views` | [overview/release-notes-v0-3-0.md#web-ui-views](../../overview/release-notes-v0-3-0.md#web-ui-views) |
  | `### Authentication Modes` | [operations/authentication-modes-and-oidc-configuration.md#authentication-modes](../../operations/authentication-modes-and-oidc-configuration.md#authentication-modes) |
  | `### Guided Onboarding Coach` (including POA&M summary paragraph) | [overview/release-notes-v0-3-0.md#guided-onboarding-coach](../../overview/release-notes-v0-3-0.md#guided-onboarding-coach) |
  | `### Role-Based Access Control` | [overview/release-notes-v0-3-0.md#role-based-access-control](../../overview/release-notes-v0-3-0.md#role-based-access-control) |
  | `### Evaluation Cancellation & History` | [overview/release-notes-v0-3-0.md#evaluation-cancellation--history](../../overview/release-notes-v0-3-0.md#evaluation-cancellation--history) |
  | `### CVE Count Accuracy` | [overview/release-notes-v0-3-0.md#cve-count-accuracy](../../overview/release-notes-v0-3-0.md#cve-count-accuracy) |
  | `### Testing Infrastructure` | [overview/release-notes-v0-3-0.md#testing-infrastructure](../../overview/release-notes-v0-3-0.md#testing-infrastructure) |
  | `## What is Crystal Forge?` | [overview/project-introduction-and-key-features.md#what-is-crystal-forge](../../overview/project-introduction-and-key-features.md#what-is-crystal-forge) |
  | `## Key Features` (four H3 groups) | [overview/project-introduction-and-key-features.md#key-features](../../overview/project-introduction-and-key-features.md#key-features) |
  | `## Architecture` | [architecture/ecosystem-architecture-summary.md](../../architecture/ecosystem-architecture-summary.md) (section rewritten during cleanup; see [cleanup record](../cleanup-record.md)) |
  | `### Components` | [architecture/ecosystem-architecture-summary.md#components](../../architecture/ecosystem-architecture-summary.md#components) |
  | `## Quick Start` (introduction) | [operations/nixos-module-configuration.md](../../operations/nixos-module-configuration.md); `README.md` holds a new quick start pointer |
  | `### NixOS Module Configuration` | [operations/nixos-module-configuration.md#nixos-module-configuration](../../operations/nixos-module-configuration.md#nixos-module-configuration) |
  | `### OIDC Configuration Example` | [operations/authentication-modes-and-oidc-configuration.md#oidc-configuration-example](../../operations/authentication-modes-and-oidc-configuration.md#oidc-configuration-example) |
  | `### Environment Variables` | [operations/authentication-modes-and-oidc-configuration.md#environment-variables](../../operations/authentication-modes-and-oidc-configuration.md#environment-variables) |
  | `## Development` | [operations/development-environment-commands.md#development](../../operations/development-environment-commands.md#development) |
  | `### Web UI Development` | [operations/development-environment-commands.md#web-ui-development](../../operations/development-environment-commands.md#web-ui-development) |
  | `### Testing` (including screenshot refresh) | [operations/development-environment-commands.md#testing](../../operations/development-environment-commands.md#testing) |
  | `## STIG Compliance Modules` | [operations/nixos-module-configuration.md#stig-compliance-modules](../../operations/nixos-module-configuration.md#stig-compliance-modules) |
  | `## Data Model` | [overview/project-introduction-and-key-features.md#data-model](../../overview/project-introduction-and-key-features.md#data-model) |
  | `## Security Model` | [overview/project-introduction-and-key-features.md#security-model](../../overview/project-introduction-and-key-features.md#security-model) |
  | `## Roadmap` and `### Active Milestones` | [overview/release-roadmap-and-milestones.md#roadmap](../../overview/release-roadmap-and-milestones.md#roadmap) |
  | `## Contributing` | `README.md` (`## Contributing`) |
  | `## License` | `README.md` (`## License`) |
- Unmapped content: none. The audit against the union of the seven concepts and `README.md` found 0 missing lines. The lines kept only in the new `README.md` are the logo and tagline markup, the `Quick Start`, `Contributing`, and `License` headings, the AGENTS.md pointer, and the license pointer.
