---
type: Design Specification
title: Release roadmap and active milestones
description: Lists the version-by-version release roadmap (v0.1.0 to v0.5.0 and future Tvix work), the progress figure, and the active Backlog milestones as recorded in the README at v0.3.0, with the v0.4.0 row updated for the release; open it to see the release plan.
tags:
  - crystal-forge
  - roadmap
  - milestones
  - overview
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T08:25:07-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file README.md at commit 3b23d36f"
    title: Crystal Forge README
---

# Release roadmap and active milestones

> **Status:** partial. This concept holds the `Roadmap` section of the repository `README.md` at commit 3b23d36f. The rows for v0.1.0 to v0.3.0, v0.5.0, and Future, and the milestone list, are a dated snapshot from v0.3.0. They are verification candidates against the Backlog (`backlog/tasks/`, `backlog/milestones/`). The v0.4.0 row was updated for the v0.4.0 release (TASK-540) from `CHANGELOG.md`. The v0.3.0 progress figure (`40% complete (59/147 tasks done)`) was removed because it was not recomputed.

## Roadmap

| Version | Status      | Features                                                                        |
| ------- | ----------- | ------------------------------------------------------------------------------- |
| v0.1.0  | Done        | Core monitoring, build coordination, CVE scanning                               |
| v0.2.0  | Done        | Deployment execution, policy enforcement, generations                           |
| v0.3.0  | Done        | Web UI (functional, not fully polished), OIDC auth, RBAC, eval cancel + history |
| v0.4.0  | In release  | Web UI design parity, CF-XCCDF interchange, CVE scan loop, Niks3 caches, access tokens |
| v0.5.0  | Backlog     | Multi-tenant support                                                            |
| Future  | Backlog     | Tvix integration                                                                |

### Active Milestones

1. **m-0**: Critical bugs and stability
2. **m-1**: Development infrastructure
3. **m-2**: Code quality and architecture (refactoring)
4. **m-3**: User interface foundation
5. **m-14**: Identity and access management

## Related concepts

- [Crystal Forge roadmap](roadmap.md)
- [Crystal Forge v0.4.0 release notes](release-notes-v0-4-0.md)
- [Crystal Forge v0.3.0 release notes](release-notes-v0-3-0.md)
- [Project introduction and key features](project-introduction-and-key-features.md)
