---
type: Reference
title: Crystal Forge v0.4.0 release notes
description: Summarizes what the v0.4.0 release adds over v0.3.1 (design-parity web UI, CF-XCCDF interchange and compliance versioning, CVE scan loop, Niks3 caches, owner access tokens with a read-only MCP endpoint, standalone NixOS module generation) and how to upgrade; open it to see the feature set of v0.4.0 and the upgrade steps.
tags:
  - crystal-forge
  - release-notes
  - overview
implementation_status: partial
generated:
  by: anthropic/claude-sonnet-5-5
  at: 2026-11-22T09:00:00-06:00
sources:
  - id: code-1
    resource: "Crystal Forge repository file packages/default/crates/cf-server/src/mcp/mod.rs at commit dd5ff524"
    title: MCP tool registry
---

# What's New in v0.4.0

> **Status:** partial. This concept describes the `dev` branch at commit `dd5ff524`, which is the base for the v0.4.0 release. It is not final until the `v0.4.0` tag exists. Work that merges after that commit is not listed. The repository `CHANGELOG.md` is the complete list with merge request numbers.

Version 0.4.0 follows `v0.3.1` (2026-05-30). The release covers 73 merge requests.

## Highlights

| Area | What changed | Read more |
| ---- | ------------ | --------- |
| Web UI | Every main view follows the updated design: shell, dashboard, Systems, System Detail, Flakes, Environments, Policies, Builds, Evaluations, CVEs, Builders, and Admin. | [Dashboard and systems views](../ui/dashboard-and-systems-views.md) |
| Compliance | CF-XCCDF interchange with compliance versioning, cross-framework reusable requirements, policy enforcement with POA&M workflows, and external or reference NixOS configuration assurance. | [CF-XCCDF interchange operator guide](../compliance/cf-xccdf-interchange-operator-guide.md) |
| NixOS modules | `cf-nixos-module` generates standalone NixOS modules from exported policies and bundles. | [NixOS module generation](../compliance/nixos-module-generation.md) |
| CVEs | A server-side scan loop runs periodic rescans. The CVEs view has multi-select filters and export formats. | [CVE concepts](../cves/index.md) |
| Caches | Public and private Niks3 caches are first-class. | [Niks3 cache](../caches/niks3-cache.md) |
| Access | Owners create access tokens in the Profile view (on by default). An optional read-only MCP endpoint serves four tools to agents (off by default). | [Owner access token service](../security/access-token-service.md), [MCP server](../security/mcp-server.md) |
| Deployment | The Deploy tab shows the flake-position lane. History shows redacted failure details and an authoritative `system_events` timeline. | [Deployment concepts](../deployment/index.md) |

## Upgrading from v0.3.1

1. Back up the PostgreSQL database. The server applies 191 new migrations (`0134` to `0336`) when it starts. No migration that existed in `v0.3.1` changed, and the migrations have no down scripts.
2. Update the `crystal-forge` NixOS module input. A comparison of the module's option declarations found no option removed since `v0.3.1`. New options cover evaluator limits, source delivery, remote execution strategy, sessions, and access tokens. See the generated NixOS options reference.
3. Access tokens are on by default (`accessTokens.enable = true`). Signed-in users can create tokens, and tokens stay inert until a consumer such as MCP admits them. Set `accessTokens.enable = false` to disable token creation. `mcp.enable` is false by default, and `POST /mcp` returns 404 while it is false. `mcp.enable` requires `accessTokens.enable`.

The `integration-upgrade` check (scenario `version-transition`) starts from the genuine `v0.3.1` packages. See [Integration coverage map](../testing/integration-coverage-map.md).

## Related concepts

- [Crystal Forge v0.3.0 release notes](release-notes-v0-3-0.md)
- [Release roadmap and active milestones](release-roadmap-and-milestones.md)
- [Project introduction and key features](project-introduction-and-key-features.md)
