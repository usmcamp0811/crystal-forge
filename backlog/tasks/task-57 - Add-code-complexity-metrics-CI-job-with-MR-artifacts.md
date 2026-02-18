---
id: TASK-57
title: Add code complexity metrics CI job with MR artifacts
assignee: []
created_date: '2026-02-18'
labels:
  - ci
  - rust
  - code-quality
  - metrics
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a GitLab CI job that computes code complexity metrics for Rust code and generates reports that are attached to merge requests as artifacts.

## Goals
- Automated complexity analysis on every MR
- Generate human-readable reports (HTML and/or Markdown)
- Reports should be accessible as CI artifacts and ideally posted as MR comments
- Track metrics like:
  - Lines of code per file/module/function
  - Cyclomatic complexity
  - Cognitive complexity (nesting depth)
  - Function length
  - Number of arguments per function
  - Code duplication detection

## Acceptance Criteria
- [ ] Create `packages/code-metrics/default.nix` with the complexity analysis script
- [ ] Use appropriate tools (rust-code-analysis, clippy lints, tokei, etc.)
- [ ] Generate HTML report artifact
- [ ] Generate Markdown summary for MR comments
- [ ] Add CI job to `.gitlab-ci.yml` that runs on merge requests
- [ ] Configure job to expose artifacts for MR review
- [ ] Reports should show per-file and per-function metrics
- [ ] Include delta/changes compared to target branch when possible
- [ ] Build passes: `nix build .#checks.x86_64-linux.code-metrics` (or similar)
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Tool Options
1. **rust-code-analysis** - Mozilla's Rust code analysis tool (supports complexity metrics)
2. **clippy** - Built-in Rust linter with complexity lints
3. **tokei** - Fast LOC counter with language breakdown
4. **scc** - Code counter with complexity estimation

### Recommended Approach
- Primary: `rust-code-analysis` for detailed complexity metrics
- Secondary: `clippy` for lint-based complexity warnings
- Output: HTML report + Markdown summary

### Artifact Structure
```
code-metrics-report/
├── index.html          # Main HTML report
├── summary.md          # Markdown summary for MR
├── per-file/
│   ├── src-main.rs.json
│   └── ...
└── summary.json        # Machine-readable summary
```

### CI Integration
- Add new stage: `metrics` (runs after `check`)
- Job should generate artifacts with `expose_as: 'Code Complexity Report'`
- Consider posting summary as MR comment using GitLab API
<!-- SECTION:NOTES:END -->
