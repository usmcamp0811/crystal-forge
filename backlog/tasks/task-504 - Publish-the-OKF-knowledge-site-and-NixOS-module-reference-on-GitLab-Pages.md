---
id: TASK-504
title: Publish the OKF knowledge site and NixOS module reference on GitLab Pages
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-24 14:34'
updated_date: '2026-10-24 14:47'
labels:
  - documentation
  - okf
  - gitlab-pages
dependencies: []
references:
  - 'https://gitlab.com/usmcamp0811/dotfiles'
  - 'https://gitlab.com/crystal-forge/crystal-forge'
documentation:
  - docs/knowledge/index.md
  - docs/knowledge/operations/nixos-module-configuration.md
  - docs/knowledge/meta/okf-conventions.md
priority: medium
type: feature
ordinal: 555000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Users need a browsable published Crystal Forge knowledge base and a reference for the project's NixOS module options without reading source files. The requested experience is similar to the user's dotfiles documentation site at https://gitlab.com/usmcamp0811/dotfiles, available locally at /home/mcamp/code/dotfiles. Publish the canonical docs/knowledge corpus, not a second independently maintained copy. This task does not change the Crystal Forge application, database, or module behavior.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 A static site presents the canonical OKF knowledge corpus with navigation and search while visibly distinguishing implemented, partial, proposed, and historical behavior.
- [ ] #2 The site includes a build-generated reference for the exported Crystal Forge NixOS module options, including service options and STIG controls/presets, with documented reference limitations.
- [ ] #3 Knowledge links, images, and Mermaid diagrams work in the published site; references to repository files outside the published corpus have an explicit usable destination.
- [ ] #4 The repository provides a reproducible Nix site build and a persistent local preview with documented build and review instructions.
- [ ] #5 GitLab CI publishes the generated static site through GitLab Pages from the default branch without publishing unrelated repository data.
- [ ] #6 The main README links to the intended GitLab Pages site, and knowledge guidance explains publication, generated options, and local verification without claiming an unverified deployment.
- [ ] #7 Automated checks cover generated option coverage, representative knowledge content and implementation-status labels, internal links/assets, and deployment-prefix-safe navigation; applicable Nix and browser checks are recorded with actual results.
- [ ] #8 The documentation site closely matches Crystal Forge's visual identity, including its purple accent, dark/light surface colors, typography, navigation, and accessible responsive controls; the user may review the theme through the persistent preview.
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. Adapt the bounded dotfiles OKF renderer and static assets with source attribution, preserving implementation_status and rewriting out-of-bundle repository references rather than silently dropping them. Do not depend on the whole dotfiles flake.
2. Generate option Markdown from disabled/schema-only evaluation of all exported Crystal Forge NixOS modules, including services and STIG namespaces; document intentional defaults/examples limitations.
3. Package the site and focused tests through the existing Nix flake. Start a task-owned loopback static HTTP preview before UI edits; refresh generated content after changes. This is not the Dioxus product UI, so no application/backend/database stack is needed.
4. Add default-branch GitLab Pages deployment, README link, and knowledge publication guidance.
5. Run focused builds, renderer/options/browser checks and required broader Nix verification. Record exact outcomes, preserve preview, and do not commit/push/open MR without user authorization.

User added CF-matching theme requirement. Keep existing bounded static renderer (Docusaurus/Material were suggestions, not required frameworks). Use actual packages/web-ui/assets/app.css dark/light tokens, CF branding, application-like sidebar/header, documentation-readable content widths, accessible theme switch, and mobile layout. Add browser smoke coverage for styling/theme/search/navigation/options/Mermaid with screenshots.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
LOCK: openai on sledge in /home/mcamp/code/crystal-forge/TASK-504-okf-site

User approved promoting TASK-504 and implementing from dev while preserving existing untracked integration-worktree files. No main worktree is registered. Dedicated branch/worktree created from dev at 1ed0b615b18258e61ea6703c342c9751000501d6; initial task-worktree status clean.

Sprint readiness: Goal is one published static knowledge/options site. Non-goals: application/Dioxus, database, module behavior, whole-dotfiles dependency, and corpus reorganization. Boundaries: source-controlled knowledge remains canonical; schema-only option evaluation; no real host or secrets; static-only preview with no backend/database. Impact: docs-site package/check, flake export, GitLab CI, README, knowledge guidance. Risk: medium (module laziness, links, Mermaid rendering, Pages URL). No external task dependencies; existing corpus is already in dev. Verification: focused renderer/options tests, Nix site and knowledge builds, browser smoke/prefix checks, and nix flake check --keep-going (report actual blockers).

Renderer and options slices implemented by delegated agents. Options report 31 module exports and 263 options; targeted package build succeeded before final integration. Owner must reverify integrated source and browser. Preview server task504-docs currently serves task worktree at 127.0.0.1:8754; not yet publishing rendered site.
<!-- SECTION:NOTES:END -->
