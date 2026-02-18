---
id: TASK-58
title: Add code coverage CI job with MR artifacts
status: Done
assignee: []
created_date: '2026-02-18'
updated_date: '2026-02-18'
labels:
  - ci
  - rust
  - code-quality
  - coverage
  - testing
dependencies:
  - TASK-57
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create a GitLab CI job that computes code coverage for Rust tests and generates reports that are attached to merge requests as artifacts.

## Goals
- Automated coverage analysis on every MR
- Generate human-readable reports (HTML)
- Show coverage delta compared to target branch
- Track coverage trends over time
- Identify uncovered code paths

## Acceptance Criteria
- [x] Create `packages/coverage/default.nix` with the coverage analysis script
- [x] Use `cargo-tarpaulin` for coverage collection
- [x] Generate HTML coverage report artifact
- [x] Generate coverage summary in Markdown format
- [x] Add CI job to `.gitlab-ci.yml` that runs on merge requests
- [x] Configure job to expose artifacts for MR review
- [x] Show line-by-line coverage in HTML report
- [x] Include coverage percentage in summary
- [x] Build passes: `nix build .#packages.x86_64-linux.coverage`
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Completion Summary (2026-02-18)

### Tools Used
- **cargo-tarpaulin** - Popular Rust coverage tool with HTML and JSON output
- **jq** - JSON processing for report generation
- **bc** - Mathematical calculations for percentages

### Files Created
- `packages/coverage/default.nix` - Nix package with coverage-report script

### Files Modified
- `.gitlab-ci.yml` - Added `coverage` stage with `coverage-check` job and MR comment posting

### Features
1. **Coverage Analysis** - Uses cargo-tarpaulin to analyze test coverage
2. **HTML Report** - Detailed per-file coverage reports
3. **Markdown Summary** - For posting as MR comments
4. **JUnit XML Report** - For GitLab test report integration
5. **CI Integration**:
   - Runs on every MR
   - Exposes artifacts as 'Code Coverage Report'
   - Posts summary as MR comment
   - Warning if coverage is below 60% (but doesn't fail)

### Usage
```bash
# Run locally
nix run .#packages.x86_64-linux.coverage

# Or with custom output directory
PROJECT_ROOT=/path/to/project OUTPUT_DIR=/path/to/output coverage-report
```

### Report Outputs
- `coverage-report/index.html` - Main HTML coverage report
- `coverage-report/summary.md` - Markdown summary for MR
- `coverage-report/coverage.json` - Machine-readable coverage data
- `coverage-report/junit-report.xml` - JUnit XML for GitLab
- `coverage-report/packages/...` - Per-package detailed reports

### CI Jobs
1. **coverage-check** - Runs on merge requests and main, generates reports as artifacts
2. **.coverage-mr-comment** (hidden) - Posts the summary as an MR comment
<!-- SECTION:NOTES:END -->
