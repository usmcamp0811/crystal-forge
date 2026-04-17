---
id: TASK-139
title: Optimize Nix Checks - Reduce CI Check Count and Execution Time
status: Backlog
assignee: []
created_date: '2026-02-28 02:51'
labels:
  - performance
  - ci
  - nix
  - optimization
  - developer-experience
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit and optimize the current Nix flake checks to minimize redundant VM standup/teardown cycles and reduce overall CI execution time while maintaining comprehensive test coverage.

## Context

Currently, our CI runs multiple separate flake checks, each requiring:
- Full NixOS VM initialization
- Database setup
- Service startup
- Network configuration
- Test execution
- VM teardown

This happens **repeatedly** for each check, leading to:
- Long CI pipeline execution times
- Wasted compute resources
- Slower developer feedback loops
- Higher CI costs (if applicable)

## Current State Analysis

As of the latest CI configuration, we have:

**Parallel flake-check jobs** (8 separate VMs):
- `flake-check: [attic_cache]`
- `flake-check: [builder]`
- `flake-check: [dashboard]`
- `flake-check: [database]`
- `flake-check: [oidc-auth]`
- `flake-check: [s3_cache]`
- `flake-check: [server]`
- `flake-check: [web-ui]`

**Additional standalone checks**:
- `codex-code-review` (AI review)
- `complexity-check` (code metrics)
- `coverage-check` (test coverage)

### Questions to Answer

1. **Can we combine multiple checks into a single VM?**
   - Example: Run database + server + web-ui tests in one VM
   - Shared fixtures could eliminate redundant setup

2. **Which checks are truly independent?**
   - builder vs server vs database
   - Do they share common infrastructure needs?

3. **Can we use multi-stage testing within a single VM?**
   - Stage 1: Unit tests (fast, no VM)
   - Stage 2: Integration tests (single VM, multiple services)
   - Stage 3: E2E tests (only if needed)

4. **Are all checks equally important for every commit?**
   - Can some checks be gated (only run on MRs, not every push)?
   - Can some be scheduled nightly instead of per-commit?

5. **Can we leverage Nix build caching better?**
   - Are we rebuilding the same derivations multiple times?
   - Can we share build artifacts between checks?

## Proposed Investigation

### Phase 1: Audit Current Checks (1-2 days)

1. **Document each check's purpose**
   - What does it test?
   - What infrastructure does it need?
   - How long does it take?
   - What's the failure rate?

2. **Map dependencies between checks**
   - Which checks share VMs/databases/services?
   - Which checks could run in parallel safely?
   - Which checks must run sequentially?

3. **Identify redundant setup**
   - Count how many times we start PostgreSQL
   - Count how many times we initialize the database schema
   - Count how many times we build the same packages

4. **Benchmark current performance**
   - Total CI time (serial)
   - Total CI time (parallel)
   - Time per check
   - Resource usage per check

### Phase 2: Design Optimized Check Structure (2-3 days)

Explore these optimization strategies:

#### Strategy A: Consolidated Integration VM
Create a single "integration-test" VM that runs:
- Database tests
- Server API tests
- Agent integration tests
- Web UI tests
- OIDC auth tests

**Pros**: 
- Single VM standup
- Shared fixtures
- Faster overall

**Cons**: 
- Harder to debug failures
- Less granular CI feedback
- One failure blocks all tests

#### Strategy B: Grouped Checks by Layer
Group checks by architectural layer:
- **data-layer**: database, migrations, queries
- **service-layer**: server, API, OIDC, builder
- **client-layer**: web-ui, agent, dashboard

**Pros**: 
- Logical grouping
- Some parallelization retained
- Clear failure attribution

**Cons**: 
- Still requires multiple VMs
- Some redundancy remains

#### Strategy C: Fast/Slow Split
- **fast-check**: Unit tests, lints, type checks (no VM)
- **integration-check**: Single VM with all integration tests
- **e2e-check**: Full system tests (optional, gated)

**Pros**: 
- Quick feedback for common failures
- Comprehensive testing still available
- Can skip slow tests for draft MRs

**Cons**: 
- Requires refactoring existing tests
- May miss integration issues in fast check

#### Strategy D: Incremental Testing
Use Nix's dependency tracking to only run checks for changed components:
- If only UI changed → only web-ui check
- If backend changed → server + database + agent checks
- If common code changed → all checks

**Pros**: 
- Minimal checks per commit
- Very fast for small changes

**Cons**: 
- Complex to implement
- May miss cross-component issues
- Requires accurate dependency graph

### Phase 3: Implement Optimizations (3-5 days)

Based on Phase 2 findings, implement the chosen strategy:

1. **Refactor flake checks**
   - Combine checks where appropriate
   - Remove redundant setup code
   - Share common fixtures

2. **Update CI configuration**
   - Adjust `.gitlab-ci.yml` to reflect new check structure
   - Update parallel matrix if needed
   - Add conditional check execution

3. **Preserve test coverage**
   - Ensure no tests are lost in consolidation
   - Maintain same test assertions
   - Keep failure granularity where critical

4. **Update documentation**
   - Document new check structure
   - Explain when each check runs
   - Provide debugging guide

### Phase 4: Validate & Benchmark (1 day)

1. **Run full CI suite**
   - Verify all tests still pass
   - Ensure no regressions

2. **Benchmark improvements**
   - Compare old vs new CI times
   - Measure resource usage reduction
   - Calculate cost savings (if applicable)

3. **Monitor for issues**
   - Watch for flaky tests
   - Track failure attribution
   - Gather developer feedback

## Success Criteria

### Primary Goals
- [ ] Reduce total CI execution time by at least 30%
- [ ] Reduce number of VM standups by at least 50%
- [ ] Maintain or improve test coverage
- [ ] Preserve failure granularity for debugging

### Secondary Goals
- [ ] Reduce CI resource usage (CPU, memory, disk)
- [ ] Improve developer feedback loop (faster failures)
- [ ] Make CI configuration more maintainable
- [ ] Document optimization patterns for future checks

## Constraints

### Must Preserve
- ✅ All existing test coverage
- ✅ Ability to identify which component failed
- ✅ Parallel execution where beneficial
- ✅ Reproducible Nix builds

### Can Change
- ❌ Number of check jobs in CI
- ❌ VM configuration and layout
- ❌ Check execution order
- ❌ Fixture sharing strategy

## Example Optimization

**Before** (8 separate VMs):
```nix
checks = {
  database = nixosTest { ... };      # VM #1
  server = nixosTest { ... };        # VM #2  
  web-ui = nixosTest { ... };        # VM #3
  # ... 5 more VMs
};
```

**After** (consolidated):
```nix
checks = {
  # Fast checks (no VM)
  lint = runCommand { ... };
  type-check = runCommand { ... };
  
  # Integration suite (single VM, multiple test suites)
  integration = nixosTest {
    testScript = ''
      # All integration tests in one VM
      run_database_tests()
      run_server_tests()
      run_web_ui_tests()
      run_agent_tests()
    '';
  };
  
  # Optional: Keep critical checks separate for fast failure
  security-pentest = nixosTest { ... };  # Only on MRs
};
```

## Deliverables

1. **Analysis Report**
   - Current check inventory
   - Redundancy analysis
   - Optimization recommendations
   - Cost-benefit analysis

2. **Optimized Nix Checks**
   - Refactored `flake.nix` checks
   - Updated NixOS test modules
   - Consolidated test suites

3. **Updated CI Configuration**
   - Modified `.gitlab-ci.yml`
   - New check job definitions
   - Conditional execution rules

4. **Documentation**
   - CI optimization guide
   - Check structure explanation
   - Debugging runbook
   - Performance benchmarks

## Out of Scope

- Changing the actual test assertions (only restructuring)
- Removing test coverage (only consolidating)
- Non-Nix CI optimizations (Docker, caching, etc.)
- Flake build optimizations (separate concern)

## References

- Nix Manual: https://nixos.org/manual/nix/stable/
- NixOS Test Framework: https://nixos.org/manual/nixos/stable/index.html#sec-nixos-tests
- GitLab CI Optimization: https://docs.gitlab.com/ee/ci/pipelines/pipeline_efficiency.html
- Current CI config: `.gitlab-ci.yml`
- Current checks: `checks/` directory
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Complete audit document listing all current checks, their purpose, duration, and dependencies
- [ ] #2 Optimization strategy selected with documented rationale
- [ ] #3 Refactored nix checks reduce VM standup count by at least 50%
- [ ] #4 Total CI execution time reduced by at least 30%
- [ ] #5 All existing tests still pass with same coverage
- [ ] #6 Failure attribution remains clear (can identify which component failed)
- [ ] #7 Updated .gitlab-ci.yml with optimized check structure
- [ ] #8 Benchmark report comparing before/after performance
- [ ] #9 Documentation updated with new check structure and debugging guide
- [ ] #10 Developer feedback collected and positive
<!-- AC:END -->

## Definition of Done
<!-- DOD:BEGIN -->
- [ ] #1 CI runs successfully on test MR with optimized checks
- [ ] #2 Performance benchmarks show measurable improvement
- [ ] #3 No test coverage regressions detected
- [ ] #4 Team review confirms improved developer experience
<!-- DOD:END -->
