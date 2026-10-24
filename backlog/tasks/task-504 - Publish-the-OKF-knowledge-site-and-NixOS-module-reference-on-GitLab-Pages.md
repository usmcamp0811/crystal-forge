---
id: TASK-504
title: Publish the OKF knowledge site and NixOS module reference on GitLab Pages
status: In Progress
assignee:
  - '@openai'
created_date: '2026-10-24 14:34'
updated_date: '2026-10-24 15:22'
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
modified_files:
  - .gitlab-ci.yml
  - README.md
  - flake.nix
  - checks/docs-site/default.nix
  - checks/docs-site/run.py
  - docs/knowledge/meta/documentation-site.md
  - docs/knowledge/meta/index.md
  - docs/knowledge/meta/migration-manifest/testing-operations.md
  - packages/docs-site/README.md
  - packages/docs-site/assets/app.js
  - packages/docs-site/assets/style.css
  - packages/docs-site/assets/theme.js
  - packages/docs-site/browser_test.py
  - packages/docs-site/build.py
  - packages/docs-site/default.nix
  - packages/docs-site/options.nix
  - packages/docs-site/options.py
  - packages/docs-site/test_build.py
  - packages/docs-site/test_options.py
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
- [x] #1 A static site presents the canonical OKF knowledge corpus with navigation and search while visibly distinguishing implemented, partial, proposed, and historical behavior.
- [x] #2 The site includes a build-generated reference for the exported Crystal Forge NixOS module options, including service options and STIG controls/presets, with documented reference limitations.
- [x] #3 Knowledge links, images, and Mermaid diagrams work in the published site; references to repository files outside the published corpus have an explicit usable destination.
- [x] #4 The repository provides a reproducible Nix site build and a persistent local preview with documented build and review instructions.
- [ ] #5 GitLab CI publishes the generated static site through GitLab Pages from the default branch without publishing unrelated repository data.
- [x] #6 The main README links to the intended GitLab Pages site, and knowledge guidance explains publication, generated options, and local verification without claiming an unverified deployment.
- [ ] #7 Automated checks cover generated option coverage, representative knowledge content and implementation-status labels, internal links/assets, and deployment-prefix-safe navigation; applicable Nix and browser checks are recorded with actual results.
- [x] #8 The documentation site closely matches Crystal Forge's visual identity, including its purple accent, dark/light surface colors, typography, navigation, and accessible responsive controls; the user may review the theme through the persistent preview.
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

User explicitly authorized the dotfiles renderer adaptation under Crystal Forge's AGPLv3 license with attribution. Package provenance notice updated accordingly.

GitLab public project API confirms default_branch=dev. Pages and documentation check rules use CI_DEFAULT_BRANCH instead of assuming main. Intended conventional project Pages URL is https://crystal-forge.gitlab.io/crystal-forge/; deployed Pages URL remains unverified.

Browser subagent verified root and /project/site/ navigation, both themes, mobile controls, search, options, and all 80 published Mermaid diagrams on 41 pages in both themes at both mounts (320 renders). Artifacts from that intermediate build: /nix/store/716rr1afx82gx93z0xpl13gsiin5yn34-crystal-forge-docs-site-browser/screenshots. Final integrated source still requires rerun.

Final focused verification: `nix build .#docs-site -L --out-link /tmp/opencode/task504-site --builders '' --max-jobs 1 --cores 2` exit 0; 278 pages, 26 referenced images plus branding, zero emitted local target/prefix errors. Real generated reference has 263 options from all 31 exported modules. All 18 renderer/options tests pass (no skips in Nix build).

`nix build .#checks.x86_64-linux.docs-site .#checks.x86_64-linux.okf-knowledge -L --no-link --print-out-paths --builders '' --max-jobs 1 --cores 2` exit 0. Browser root/prefix smoke and all 80 Mermaid diagrams on 41 pages in both themes/mounts passed. Knowledge gate: 58 tests, 216 parsed Mermaid blocks, 191 concepts validated. Final browser evidence: /nix/store/z0ncppbcmibjmlm0y4glpfbq148ncjsg-crystal-forge-docs-site-browser/screenshots/ .

Initial integration failures corrected within scope: stale attribution-license test after user AGPL authorization; invalid new concept taxonomy/tag. Independent read-only review found stale license test and fixed-count brittleness; both corrected. Preset/tracking expectations now derive from export manifest rather than exact 25/4 counts.

CI YAML parsed and local assertions checked publisher/public artifact/dependencies/default-branch rules using the package's Nix Python environment; not a GitLab server-side CI lint or executed pipeline. `git diff --check` and `git diff --cached --check` exit 0. Documentation inspected for current semantics, omission limitations, static publication boundary, preview lifecycle, source links, theme, and unverified deployment claims. No Rust/API/database/module behavior changes.

Broad `nix flake check --keep-going > /tmp/opencode/task504-flake-check.log 2>&1` was interrupted by the tool's 120000 ms timeout during unrelated integration dependency fetch/build. No full-suite pass claimed. Log ends `error: interrupted by the user` due tool timeout, not an intentional manual cancellation. Retry with a longer execution budget before Review. AC7 remains unchecked pending this requirement; AC5 remains unchecked pending real Pages pipeline/deployment.

Preview ready: http://127.0.0.1:8754/ on sledge. tmux task504-docs pane PID 27024 owns current static HTTP server; launcher /tmp/opencode/task504-site/bin/docs-site 8754 serves final immutable /nix/store/r5p394q082q87r59p5d1sqnmza8l9rx8-docs-site-1/share/docs-site. HTTP home and knowledge/meta/documentation-site.html both 200. Review Knowledge, NixOS options, Search, Light theme, and mobile Browse docs. Static data only; no backend, login or database. Logs in tmux pane; no application session touched. Forward with ssh -L 8754:127.0.0.1:8754 sledge if needed.

All 19 intended files are staged to support Git-backed Nix builds. No commit, push, MR, or deployment performed. Task remains In Progress, lock retained, preview left running.
<!-- SECTION:NOTES:END -->
