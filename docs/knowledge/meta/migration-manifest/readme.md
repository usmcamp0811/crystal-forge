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
  | `## What's New in v0.3.0` | [overview/release-notes-v0-3-0.md#whats-new-in-v030](../../overview/release-notes-v0-3-0.md#whats-new-in-v030), [operations/authentication-modes-and-oidc-configuration.md#authentication-modes](../../operations/authentication-modes-and-oidc-configuration.md#authentication-modes) |
  | `## What is Crystal Forge?` | [overview/project-introduction-and-key-features.md#what-is-crystal-forge](../../overview/project-introduction-and-key-features.md#what-is-crystal-forge) |
  | `## Key Features` | [overview/project-introduction-and-key-features.md#key-features](../../overview/project-introduction-and-key-features.md#key-features) |
  | `## Architecture` | [architecture/ecosystem-architecture-summary.md#components](../../architecture/ecosystem-architecture-summary.md#components) |
  | `## Quick Start` | [operations/nixos-module-configuration.md#quick-start](../../operations/nixos-module-configuration.md#quick-start), [README.md#quick-start](../../../../README.md#quick-start) |
  | `## Development` | [operations/development-environment-commands.md#development](../../operations/development-environment-commands.md#development) |
  | `## STIG Compliance Modules` | [operations/nixos-module-configuration.md#stig-compliance-modules](../../operations/nixos-module-configuration.md#stig-compliance-modules) |
  | `## Data Model` | [overview/project-introduction-and-key-features.md#data-model](../../overview/project-introduction-and-key-features.md#data-model) |
  | `## Security Model` | [overview/project-introduction-and-key-features.md#security-model](../../overview/project-introduction-and-key-features.md#security-model) |
  | `## Roadmap` | [overview/release-roadmap-and-milestones.md#roadmap](../../overview/release-roadmap-and-milestones.md#roadmap) |
  | `## Contributing` | [README.md#contributing](../../../../README.md#contributing) |
  | `## License` | [README.md#license](../../../../README.md#license) |
- Unmapped content: none. The audit against the union of the seven concepts and `README.md` found 0 missing lines. The lines kept only in the new `README.md` are the logo and tagline markup, the `Quick Start`, `Contributing`, and `License` headings, the AGENTS.md pointer, and the license pointer.
