---
type: Reference
title: "Crystal Forge product vision pitch deck"
description: "Lists the Slidev pitch deck source files in packages/slides with a one-line summary and link for each slide; open it to find the slide that states a product claim before reusing or updating the deck."
tags:
  - crystal-forge
  - overview
  - slides
  - pitch
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:19-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file packages/slides/slides.md at commit 3b23d36f"
    title: "Crystal Forge Slidev deck (slides.md and slides/*.md)"
---

# Crystal Forge Product Vision Pitch Deck

The pitch deck is Slidev presentation source. It stays in `packages/slides/` because the slides Nix package builds it from those paths. This concept is the navigation record for the deck. The slide files remain the authoritative text.

> **Status:** The deck is marketing and vision material. It mixes current behavior with future plans (for example OSCAL export, STIG modules, Kubernetes agents). Verification candidates are listed at the end. The deck was not rewritten and was not compared with the implementation in this phase.

## Deck structure

[slides.md](../../../packages/slides/slides.md) is the entry point. It sets the Neversink theme and Crystal Forge styling and contains these inline slides:

- A cover slide with the Crystal Forge logo, the tagline "Forging Trust Through Reproducibility", and the credit "Created by Matt Camp - 2025".
- Eight section divider slides: "The Problem Space", "A Foundation built on Nix", "The Crystal Forge Solution", "Architecture & Components", "Key Advantages", "Compliance & Reporting", "What Crystal Forge Is Not", and "Closing Thoughts". Each has a one-line subtitle.

It includes the slide files below in order with the `src:` frontmatter key.

## Slide files

| Slide file | Content |
| --- | --- |
| [slides/00-title.md](../../../packages/slides/slides/00-title.md) | Title slide for a separate talk, "Nix: Taming the Wild West of Codebases" ("A Nix Powered DevSecOps Revolution"). `slides.md` does not include it. |
| [slides/01-intro.md](../../../packages/slides/slides/01-intro.md) | "What is Crystal Forge?" with a BLUF: Crystal Forge compares what runs on a NixOS fleet with what should run, and compliance becomes a database query. |
| [slides/02-compliance-burden.md](../../../packages/slides/slides/02-compliance-burden.md) | "The Compliance Burden": the question "Are all our systems actually running what we think they are?" for homelabs and large organizations. |
| [slides/03-current-approach.md](../../../packages/slides/slides/03-current-approach.md) | "The Current Approach": sysadmins, security teams, and compliance officers work in silos, and the "Don't Touch It" problem after an ATO. |
| [slides/04-traditional-tools.md](../../../packages/slides/slides/04-traditional-tools.md) | "Why Traditional Tools Fall Short": Ansible, Chef, and Puppet are imperative, order-dependent, and allow configuration drift. |
| [slides/05-nix-changes-the-game.md](../../../packages/slides/slides/05-nix-changes-the-game.md) | "Nix Changes the Game": deterministic builds and hash-based identity, shown with `readlink /run/current-system`. |
| [slides/06-nix-single-source.md](../../../packages/slides/slides/06-nix-single-source.md) | "A Single Source of Truth": derivation paths as cryptographic identities and Nix refusing ambiguous module definitions. |
| [slides/07-cf-what-if-we-made-this-simple.md](../../../packages/slides/slides/07-cf-what-if-we-made-this-simple.md) | "What If We Made This Simple?": log each system's evaluated derivation path and compare actual with expected in one lookup. |
| [slides/08-cf-who-benefits.md](../../../packages/slides/slides/08-cf-who-benefits.md) | "Who Benefits": the value for CTOs, sysadmins, security teams, and compliance officers. |
| [slides/09-cf-how-it-works.md](../../../packages/slides/slides/09-cf-how-it-works.md) | "How It Works": agent, server, and builder roles, a 15-minute heartbeat, and a sample heartbeat and target exchange. |
| [slides/10-cf-agent-lifecycle.md](../../../packages/slides/slides/10-cf-agent-lifecycle.md) | "Agent Lifecycle": signed heartbeats, verified instructions, atomic switch with rollback, self-updates, and possible Kubernetes or system-manager agents. |
| [slides/11-cf-build-coordination.md](../../../packages/slides/slides/11-cf-build-coordination.md) | "Build Coordination": distributed builder workers, vulnix scans on every closure, shared caches, and possible Kubernetes builders. |
| [slides/12-cf-beyond-config-mgmt.md](../../../packages/slides/slides/12-cf-beyond-config-mgmt.md) | "Beyond Configuration Management": declarative build-verify-deploy, built-in drift detection, and bit-for-bit reproducible sandboxes. |
| [slides/13-cf-immutable-by-desding.md](../../../packages/slides/slides/13-cf-immutable-by-desding.md) | "Immutable by Design": configuration changes as recorded events, a cryptographic audit trail, and future STIG modules and policy exception tracking. |
| [slides/14-cf-built-for-audits.md](../../../packages/slides/slides/14-cf-built-for-audits.md) | "Built for Audits": RMF and STIG mapping, framework-agnostic controls, and future framework-aware deployment policies. |
| [slides/15-cf-reporting.md](../../../packages/slides/slides/15-cf-reporting.md) | "Reporting": OSCAL, JSON, CSV, and PDF output, integrations with Splunk, Tenable, Nessus, and OpenRMF, and a future reporting dashboard. |
| [slides/16-scope-boundaries.md](../../../packages/slides/slides/16-scope-boundaries.md) | "Scope & Boundaries": configuration compliance and attestation are in scope, runtime security monitoring is out of scope. |
| [slides/17-vision.md](../../../packages/slides/slides/17-vision.md) | "The Vision": deterministic infrastructure meets compliance, and Nix becomes as common in secure environments as RedHat. |
| [slides/18-get-involved.md](../../../packages/slides/slides/18-get-involved.md) | "Get Involved": project status, roadmap (web UI, reporting, STIG and RMF policy modules), and contribution and contact links. |

## Verification candidates

- `slides/09-cf-how-it-works.md` shows `POST /api/heartbeat`, but the server routes `/agent/heartbeat` (`packages/default/crates/cf-server/src/bin/server.rs`).
- `slides/09-cf-how-it-works.md` and `slides/11-cf-build-coordination.md` state that builders talk to the database directly. The repository boundary is that API-only builders use the builder API (`packages/default/crates/cf-builder/src/builder/api_client.rs`).
- `slides/18-get-involved.md` lists the web UI as roadmap work, and a web UI exists (`packages/web-ui`).

## Related concepts

- [Problem statement](problem-statement.md) - the written problem brief behind the deck.
- [Roadmap](roadmap.md) - planned work referenced in the deck.
- [System overview](system-overview.md) - the current product description.
