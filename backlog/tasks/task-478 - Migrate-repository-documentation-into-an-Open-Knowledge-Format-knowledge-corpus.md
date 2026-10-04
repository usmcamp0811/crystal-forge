---
id: TASK-478
title: >-
  Migrate repository documentation into an Open Knowledge Format knowledge
  corpus
status: In Progress
assignee:
  - claude-agent
created_date: '2026-10-04 03:43'
updated_date: '2026-10-04 03:43'
labels:
  - documentation
  - okf
  - knowledge-base
dependencies: []
references:
  - 'https://github.com/GoogleCloudPlatform/open-knowledge-format'
priority: medium
type: docs
ordinal: 499000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Crystal Forge documentation is spread across docs/, root Markdown files, package READMEs and backlog/docs, grouped by when it was written rather than by concept. People and agents have no single entry point, related concepts are not linked, and design intent, implemented behavior and historical notes are not distinguished. The project wants one source-controlled corpus that follows the Open Knowledge Format (https://github.com/GoogleCloudPlatform/open-knowledge-format), with docs/knowledge/index.md as the progressive-disclosure entry point. This is a lossless refactor: every substantive section of every existing human-authored document must keep a destination, and no detail may be reduced to a generic summary. Agent-control files (AGENTS.md, CLAUDE.md, docs/agents/*) are operational files, not knowledge concepts, and are out of scope for migration. Implementation code must not be changed to match old documentation. Where documentation and code disagree, the corpus must state whether the documentation is stale, the implementation is incomplete relative to an approved design, the text is future behavior, the text is historical, or the code has a defect.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 The pinned upstream OKF version and the exact upstream commit SHA of SPEC.md are recorded in a migration/conventions document, and the corpus conforms to that revision. The specification text is not copied into the repository.
- [ ] #2 A source-to-destination inventory exists in a migration manifest. Every in-scope source document lists its destination(s), action (moved, split, merged, retained, replaced) and Coverage: complete. No source is partial, unresolved, unmapped or unreviewed.
- [ ] #3 docs/knowledge/index.md declares the pinned OKF version as the specification permits and links to every directory index. Each directory index describes its contents with a purpose-bearing description per link. Directory indexes carry no arbitrary frontmatter.
- [ ] #4 Every concept document has valid YAML frontmatter with the required type, uses a type from a small documented Crystal Forge taxonomy, and carries implementation_status (implemented, partial, proposed, historical) where the concept describes behavior. OKF standard fields are used with their specified semantics. Provenance records the original repository-relative source and does not fabricate authors, reviews, verification or timestamps.
- [ ] #5 Documents were moved with git mv where the mapping is clean. Documents mixing independent concepts were split without losing substantive content. Duplicate canonical documentation does not remain unintentionally. Retained README files are short pointers to the canonical concepts.
- [ ] #6 Existing Mermaid and other useful diagrams are preserved next to their owning concepts, and image and diagram paths are updated.
- [ ] #7 Implemented-behavior claims were verified against current code for deployment policy semantics, evaluation, builder, agent, binary cache, system current-state selection, CVE current-versus-historical and reconciliation, compliance assignment, report-only behavior, POA&M, sessions and API boundaries. Proposed and historical designs are preserved and labeled, not rewritten as current behavior. Disagreements are recorded explicitly.
- [ ] #8 A deterministic validator runs through the repository's Nix flake checks. It validates frontmatter, required type, reserved filenames, root index version declaration, relative links, referenced local images and diagrams, manifest completeness and absence of absolute local paths. Existing documentation-related checks still pass.
- [ ] #9 References in remaining repository files (README.md, TESTING.md, CONTRIBUTING.md, flake check READMEs, AGENTS.md pointers) resolve to the new locations. AGENTS.md and CLAUDE.md execution instructions are not converted into knowledge concepts.
- [ ] #10 A final audit report records files migrated, split, merged and retained, stale documentation corrected, unresolved conflicts, and validation performed, with the real commands and exit statuses.
<!-- AC:END -->
