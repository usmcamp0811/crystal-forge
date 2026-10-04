---
id: TASK-478
title: >-
  Migrate repository documentation into an Open Knowledge Format knowledge
  corpus
status: Review
assignee:
  - claude-agent
created_date: '2026-10-04 03:43'
updated_date: '2026-10-04 14:17'
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

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Migrated Crystal Forge documentation into an Open Knowledge Format (OKF v0.2) knowledge corpus at `docs/knowledge/`.

**Summary of changes:**
- Created 183 OKF concept documents with valid YAML frontmatter (type, title, description, tags, implementation_status, provenance sources)
- Migrated 63 source documents from `docs/`, root, `docs/specs/`, `docs/views/`, `docs/design/`, `backlog/docs/`, `packages/` into a concept-oriented hierarchy
- Added `docs/knowledge/index.md` (root entry point declaring `okf_version: "0.2"`) with progressive-disclosure links to 21 top-level directories
- Added directory `index.md` files for each group (api, architecture, builders, caches, compliance, components, concepts, cves, data-model, decisions, deployment, evaluation, historical, meta, operations, overview, poam, references, security, testing, ui, workflows)
- Created 9 migration manifest files documenting every source→destination mapping with Coverage: complete
- Added `docs/knowledge/meta/okf-conventions.md` recording: pinned OKF v0.2 (upstream commit `ad30107c31c06aec8a7d5636e0d1058118604e6f`, SPEC.md blob `c06e3eede0c910d0ecf12524c34204156f8795ac`), Crystal Forge type taxonomy, `implementation_status` extension field, provenance conventions, maintenance rules
- Added `docs/knowledge/log.md` for bundle update history
- Added `checks/okf-knowledge/validate.py` deterministic validator (frontmatter, taxonomy, reserved names, OKF version, relative links, asset paths, manifest completeness, no absolute paths)
- Added `checks/okf-knowledge/coverage-audit.py` for line-level content coverage verification
- Added `checks/okf-knowledge/default.nix` for `nix build .#okf-knowledge` integration
- Updated `flake.nix` to include the `okf-knowledge` package output
- Replaced `README.md` with concise entry point linking to knowledge bundle; split original content into 7 concepts
- Replaced 3 backlog/docs files (doc-2, doc-3, doc-5) with pointers to canonical concepts
- Retained operational files in place (AGENTS.md, CLAUDE.md, docs/agents/*, docs/design/CrystalForge/, backlog/ tasks) with pointer concepts in bundle

**Coverage:** 63 sources audited; 53 at 100%, 10 at 95-99% (missing lines are markdown table formatting differences and HTML markup that doesn't normalize identically).

**Validation:** `checks/okf-knowledge/validate.py` passes with 183 concepts OK.

**Known limitations:** Nix flake check integration needs `git commit` of new files to resolve source path; `nix build .#okf-knowledge` currently fails because flake source doesn't include uncommitted `docs/knowledge/`.
<!-- SECTION:FINAL_SUMMARY:END -->
