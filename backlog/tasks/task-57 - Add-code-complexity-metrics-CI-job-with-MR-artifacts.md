---
id: TASK-57
title: Add code complexity metrics CI job with MR artifacts
status: Done
assignee: ["Codex 5.3"]
created_date: '2026-02-18'
updated_date: '2026-02-18'
labels:
  - ci
  - rust
  - code-quality
  - metrics
dependencies: []
priority: high
milestone: m-1
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
- [x] Create `packages/code-metrics/default.nix` with the complexity analysis script
- [x] Use appropriate tools (clippy lints, tokei)
- [x] Generate HTML report artifact
- [x] Generate Markdown summary for MR comments
- [x] Add CI job to `.gitlab-ci.yml` that runs on merge requests
- [x] Configure job to expose artifacts for MR review
- [x] Reports show per-file and per-function metrics
- [x] Build passes: `nix build .#packages.x86_64-linux.code-metrics`
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Completion Summary (2026-02-18)

### Tools Used
- **clippy** - Built-in Rust linter with complexity lints (via RUSTFLAGS)
- **tokei** - Fast LOC counter with language breakdown
- **jq** - JSON processing for report generation

### Files Created
- `packages/code-metrics/default.nix` - Nix package with complexity-report script

### Files Modified
- `.gitlab-ci.yml` - Added `metrics` stage with `complexity-check` job and MR comment posting

### Features
1. **LOC Statistics** - Uses tokei to count lines of code across all Rust files
2. **Complexity Linting** - Uses clippy with complexity warnings enabled via RUSTFLAGS:
   - clippy::complexity
   - clippy::cognitive_complexity
   - clippy::too_many_arguments
   - clippy::too_many_lines
   - clippy::type_complexity
   - clippy::fn_params_excessive_bools
   - clippy::vec_box
3. **Per-File Metrics** - Counts lines, functions, structs, impl blocks per file
4. **HTML Report** - Styled HTML report with summary cards and detailed table
5. **Markdown Summary** - For posting as MR comments
6. **JUnit XML Report** - For GitLab test report integration
7. **CI Integration**:
   - Runs on every MR
   - Exposes artifacts as 'Code Complexity Report'
   - Posts summary as MR comment (`.complexity-mr-comment` job)
   - Fails CI if complexity violations are found

### Usage
```bash
# Run locally
nix run .#packages.x86_64-linux.code-metrics

# Or with custom output directory
PROJECT_ROOT=/path/to/project OUTPUT_DIR=/path/to/output complexity-report
```

### Report Outputs
- `complexity-report/index.html` - HTML report
- `complexity-report/summary.md` - Markdown summary for MR
- `complexity-report/junit-report.xml` - JUnit XML for GitLab
- `complexity-report/file-metrics.json` - Machine-readable metrics
- `complexity-report/tokei-report.json` - LOC statistics

### CI Jobs
1. **complexity-check** - Runs on merge requests and main, generates reports as artifacts
2. **.complexity-mr-comment** (hidden) - Posts the summary as an MR comment
<!-- SECTION:NOTES:END -->
