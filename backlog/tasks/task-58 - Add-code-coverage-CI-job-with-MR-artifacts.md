---
id: TASK-58
title: Add code coverage CI job with MR artifacts
assignee: []
created_date: '2026-02-18'
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
- [ ] Create `packages/coverage/default.nix` with the coverage analysis script
- [ ] Use `cargo-tarpaulin` or `grcov` for coverage collection
- [ ] Generate HTML coverage report artifact
- [ ] Generate coverage summary in Markdown format
- [ ] Add CI job to `.gitlab-ci.yml` that runs on merge requests
- [ ] Configure job to expose artifacts for MR review
- [ ] Show line-by-line coverage in HTML report
- [ ] Include coverage percentage in summary
- [ ] Show coverage delta (change from target branch)
- [ ] Build passes: `nix build .#checks.x86_64-linux.coverage` (or similar)
<!-- SECTION:DESCRIPTION:END -->

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
<!-- SECTION:NOTES:END -->
