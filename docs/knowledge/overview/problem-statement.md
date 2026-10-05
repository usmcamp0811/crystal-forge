---
type: Concept
title: "Crystal Forge problem brief"
description: "States who is hurt by missing NixOS compliance tooling, the four gaps Crystal Forge addresses, why now, what is out of scope this phase, and the success signal; open it to understand the product purpose."
tags:
  - crystal-forge
  - overview
  - problem
  - compliance
implementation_status: partial
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/problem_statement.md at commit 3b23d36f"
    title: "Crystal Forge Problem Brief"
---

# Crystal Forge Problem Brief

> **Status:** partial. The problem framing is current product intent. The success signal (cryptographic proof to auditors that running systems match declared configurations) is a goal. [System context and current state](system-context-and-current-state.md) records that the compliance story is still in progress.

## Who Is Hurt

Organizations using NixOS in controlled/regulated environments:

- **Government agencies** requiring security compliance and audit trails
- **Healthcare organizations** needing HIPAA compliance and system verification
- **Financial institutions** requiring SOX, PCI-DSS, and regulatory oversight
- **Any enterprise** adopting NixOS but lacking compliance tooling equivalent to traditional solutions

## The Problem

Current NixOS deployments in regulated environments face critical gaps:

1. **No compliance-native monitoring** - existing tools don't understand Nix's declarative model
2. **CVE scanning blind spots** - traditional scanners miss Nix package vulnerabilities
3. **Audit trail gaps** - can't prove system state matches declared configuration
4. **Distributed build limitations** - Hydra doesn't handle flakes well or distribute effectively

## Why Now

- NixOS adoption growing in enterprise but hitting compliance walls
- Regulatory pressure increasing for software supply chain security
- Traditional compliance tools (Intune, etc.) don't fit declarative infrastructure

## Out of Scope (This Phase)

- Non-NixOS systems
- Real-time incident response
- General-purpose monitoring (performance metrics, etc.)

## One Success Signal

A government agency can demonstrate to auditors that their NixOS fleet is compliant, with cryptographic proof that running systems match declared configurations and all CVEs are tracked.

## Related concepts

- [Constraints and policy](constraints-and-policy.md) - non-negotiable constraints that follow from this problem
- [System context and current state](system-context-and-current-state.md) - where the product stands today
- [Roadmap](roadmap.md) - planned work that closes these gaps
