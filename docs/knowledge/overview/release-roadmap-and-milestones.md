---
type: Design Specification
title: Release roadmap and active milestones
description: Lists the version-by-version release roadmap (v0.1.0 to v0.5.0 and future Tvix work), the progress figure, and the active Backlog milestones as recorded in the README at v0.3.0; open it to see the release plan at that time.
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

> **Status:** partial. This concept holds the `Roadmap` section of the repository `README.md` at commit 3b23d36f. The progress figure (`40% complete (59/147 tasks done)`), the version rows, and the milestone list are a dated snapshot. They are verification candidates against the Backlog (`backlog/tasks/`, `backlog/milestones/`) and the crate version `0.3.0`. The item order and statuses are not rewritten here.

## Roadmap

**Progress**: 40% complete (59/147 tasks done)

| Version | Status      | Features                                                                        |
| ------- | ----------- | ------------------------------------------------------------------------------- |
| v0.1.0  | Done        | Core monitoring, build coordination, CVE scanning                               |
| v0.2.0  | Done        | Deployment execution, policy enforcement, generations                           |
| v0.3.0  | Done        | Web UI (functional, not fully polished), OIDC auth, RBAC, eval cancel + history |
| v0.4.0  | Backlog     | UI polish, advanced compliance reporting                                        |
| v0.5.0  | Backlog     | Multi-tenant support                                                            |
| Future  | Backlog     | Tvix integration                                                                |

### Active Milestones

1. **m-0**: Critical bugs and stability
2. **m-1**: Development infrastructure
3. **m-2**: Code quality and architecture (refactoring)
4. **m-3**: User interface foundation
5. **m-14**: Identity and access management (current focus)

## Related concepts

- [Crystal Forge roadmap](roadmap.md)
- [Crystal Forge v0.3.0 release notes](release-notes-v0-3-0.md)
- [Project introduction and key features](project-introduction-and-key-features.md)
