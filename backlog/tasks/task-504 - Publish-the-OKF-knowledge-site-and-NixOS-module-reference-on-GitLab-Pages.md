---
id: TASK-504
title: Publish the OKF knowledge site and NixOS module reference on GitLab Pages
status: Backlog
assignee: []
created_date: '2026-10-24 14:34'
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
<!-- AC:END -->
