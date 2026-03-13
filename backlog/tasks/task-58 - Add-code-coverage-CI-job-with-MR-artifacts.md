---
id: TASK-58
title: Add code coverage CI job with MR artifacts
status: Done
assignee:
  - Codex 5.3
created_date: '2026-02-18'
updated_date: '2026-03-13 01:24'
labels:
  - ci
  - rust
  - code-quality
  - coverage
  - testing
milestone: m-1
dependencies:
  - TASK-57
priority: high
ordinal: 11000
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
<!-- AC:BEGIN -->
- [ ] #1 Create `packages/coverage/default.nix` with the coverage analysis script
- [ ] #2 Use `cargo-tarpaulin` or `grcov` for coverage collection
- [ ] #3 Generate HTML coverage report artifact
- [ ] #4 Generate coverage summary in Markdown format
- [ ] #5 Add CI job to `.gitlab-ci.yml` that runs on merge requests
- [ ] #6 Configure job to expose artifacts for MR review
- [ ] #7 Show line-by-line coverage in HTML report
- [ ] #8 Include coverage percentage in summary
- [ ] #9 Show coverage delta (change from target branch)
- [ ] #10 Build passes: `nix build .#checks.x86_64-linux.coverage` (or similar)
<!-- SECTION:DESCRIPTION:END -->

<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
### Tool Options
1. **cargo-tarpaulin** - Popular Rust coverage tool, supports various output formats
2. **grcov** - Mozilla's coverage tool, works with LLVM coverage
3. **cargo-llvm-cov** - Uses LLVM's native coverage instrumentation

### Recommended Approach
- Primary: `cargo-tarpaulin` (well-maintained, good Nix support)
- Output formats: HTML (detailed), JSON (machine-readable), Markdown (MR summary)

### Artifact Structure
```
coverage-report/
├── index.html          # Main HTML coverage report
├── summary.md          # Markdown summary for MR
├── coverage.json       # Machine-readable coverage data
└── detailed/           # Per-file HTML reports
    ├── src/
    │   └── main.rs.html
    └── ...
```

### CI Integration
- Add new stage: `coverage` (runs after `check`, parallel with `metrics`)
- Job should generate artifacts with `expose_as: 'Code Coverage Report'`
- Consider posting coverage summary as MR comment using GitLab API
- May need to run tests with coverage instrumentation (can be slow)

### Coverage Configuration
- Minimum coverage threshold (optional - can warn but not fail)
- Exclude test files and generated code
- Focus on production code in `src/` directories
- Line coverage and branch coverage

### Database Considerations
- Coverage tests may need PostgreSQL running
- Use the existing test infrastructure from the project
- Consider using `nix build` with coverage instead of `cargo test`

Closed from MR audit: merged in MR !109 (TASK-58-coverage-main -> main).
<!-- SECTION:NOTES:END -->
