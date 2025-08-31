# Crystal Forge Testing Plan

## Executive Summary

Crystal Forge requires comprehensive testing to ensure reliability, security, and compliance capabilities in regulated environments. This plan defines our testing strategy across unit, integration, database, and system levels using Rust's built-in testing, pytest, and NixOS VM tests.

## Testing Philosophy

- **Prove functionality**: Tests demonstrate Crystal Forge delivers on its compliance monitoring promises
- **Prevent regressions**: Comprehensive test coverage catches breaking changes before deployment
- **Document behavior**: Tests serve as executable specifications of system behavior
- **Enable confident changes**: Strong test suite allows rapid, safe development

## Testing Architecture

### Test Levels

```
┌─────────────────────────────────────────────┐
│                System Tests                 │
│         (Full VM fleet scenarios)           │
├─────────────────────────────────────────────┤
│            Integration Tests                │
│    (Cross-component interactions in VMs)    │
├─────────────────────────────────────────────┤
│             Database Tests                  │
│    (Direct DB operations & scenarios)       │
├─────────────────────────────────────────────┤
│               Unit Tests                    │
│        (Rust function-level tests)          │
└─────────────────────────────────────────────┘
```

### Test Infrastructure

- **cf-test package**: Centralized pytest framework avoiding VM test script bloat
- **Scenario builders**: Reusable database state generators for consistent test data
- **NixOS VMs**: Isolated test environments matching production deployments
- **Test markers**: pytest markers for test categorization (smoke, database, views, integration)

## File Structure & Organization

### Test Locations

```
crystal-forge/
├── checks/
│   ├── crystal-forge/
│   │   └── default.nix    # Core integration tests run from here
│   └── database/
│       └── default.nix    # Database tests run in this
├── packages/
│   ├── default/
│   │   ├── src/           # Rust source with inline unit tests
│   │   └── Cargo.toml
│   ├── cf-test-modules/
│   │   ├── cf_test/
│   │   │   ├── __init__.py
│   │   │   ├── client.py
│   │   │   ├── scenarios/
│   │   │   │   ├── __init__.py
│   │   │   │   ├── core.py
│   │   │   │   ├── single_system.py
│   │   │   │   └── multi_system.py
│   │   │   └── tests/     # All pytest tests go here
│   │   │       ├── database/
│   │   │       │   └── test_view_*.py
│   │   │       ├── test_integration_*.py
│   │   │       ├── test_fleet_*.py
│   │   │       └── test_*_smoke.py
│   │   ├── default.nix
│   │   └── pyproject.toml
│   └── ...
└── docs/
    └── test_plan.md       # This document
```

## Test Categories

### 1. Unit Tests (Rust)

**Location**: Inline with Rust source code in `packages/default/src/` using `#[cfg(test)]` modules

**Scope**: Individual functions and modules

**Examples**:

- Vulnix parser JSON handling (`vulnix_parser.rs`)
- Ed25519 signature verification
- Configuration parsing
- State fingerprint generation

**Execution**:

```bash
# Automatically run during Nix build
nix build

# Or manually with cargo
cd packages/default
cargo test
```

### 2. Database Tests

**Location**: `packages/cf-test-modules/cf_test/tests/test_view_*.py`

**Scope**: Database views, queries, and data integrity

**Key Areas**:

- View correctness (deployment status, heartbeat status, commit timelines)
- Scenario validation (behind, offline, unknown states)
- Performance benchmarks (query execution time)
- Data consistency across related tables

**Execution**: Direct database connection from server node

```bash
# Run all database tests in DevShell with DB running.
nix run .#cf-test-modules.runTests -- -vvv -m database
```

### 3. Integration Tests

**Location**: `packages/cf-test-modules/cf_test/tests/test_integration_*.py`

**Scope**: Component interactions within VMs

**Test Patterns**:

```python
def test_agent_server_communication(agent_vm, server_vm):
    # Start agent on VM
    agent_vm.execute("systemctl start crystal-forge-agent")

    # Verify heartbeat received
    result = server_vm.wait_until_succeeds(
        "curl -s localhost:3000/api/systems | jq '.systems | length'"
    )
    assert int(result) > 0
```

**Key Areas**:

- Agent → Server communication
- Git webhook processing
- Builder coordination
- CVE scanning pipeline

### 4. System Tests

**Location**: `packages/cf-test-modules/cf_test/tests/test_fleet_*.py`

**Scope**: Full fleet behavior across multiple VMs

**Scenarios**:

- Fleet-wide configuration updates
- Rolling deployments
- Failure recovery
- Network partitions
- Compliance drift detection

## Test Data Management

### Scenario System

**Location**: `packages/cf-test-modules/cf_test/scenarios/`

**Purpose**: Generate consistent, realistic test data

**Core Functions**:

- `_create_base_scenario()`: Standard flake→commit→derivation→system chain
- `scenario_*()`: Specific test conditions (behind, offline, failed builds)
- `_cleanup_fn()`: Automatic test data removal

**Usage Example**:

```python
def test_deployment_behind(cf_client, clean_test_data):
    scenario = scenario_behind(cf_client)

    rows = cf_client.execute_sql(
        "SELECT * FROM view_system_deployment_status WHERE hostname = %s",
        (scenario["hostname"],)
    )

    assert rows[0]["deployment_status"] == "behind"
```

## Test Execution Strategy

### Local Development

```bash
# Unit tests (automatic with build)
nix build

# Database tests only
nix build .#checks.x86_64-linux.database

# Full test suite
nix flake check

# Run Database tests in DevShell (`nix develop`) against development database
server-stack up
run-db-test -vvv -m database
```

### CI Pipeline

```yaml
stages:
  - unit: nix build # Unit tests run automatically
  - database: pytest -m database
  - integration: pytest -m integration
  - system: pytest -m vm_only
  - smoke: pytest -m smoke --maxfail=1
```

### Test Markers

- `@pytest.mark.smoke`: Critical path tests, run first
- `@pytest.mark.database`: Direct database operations
- `@pytest.mark.views`: Database view validation
- `@pytest.mark.integration`: Multi-component tests
- `@pytest.mark.vm_only`: Requires full VM environment
- `@pytest.mark.vm_internal`: Tests that run inside VMs
- `@pytest.mark.driver`: VM driver/control tests

## Coverage Requirements

### Minimum Coverage Targets

- **Unit tests**: 80% line coverage for core logic
- **Database views**: 100% view coverage with scenario tests
- **API endpoints**: 100% endpoint coverage
- **Critical paths**: 100% coverage for security/compliance features

### Coverage Verification

```bash
# Rust coverage
cd packages/default
cargo tarpaulin --out Html

```

_TODO: Need to figure out how to validate integration tests cover all the things they should cover_

## Test Documentation

### Test Naming Conventions

- **Unit tests**: `test_<function>_<condition>_<expected>`
- **Database tests**: `test_<view>_<scenario>`
- **Integration tests**: `test_<component>_<interaction>`
- **System tests**: `test_fleet_<behavior>`

### Test Documentation Requirements

Each test should include:

- Purpose statement
- Setup requirements
- Expected behavior
- Cleanup verification

Example:

```python
def test_deployment_rollback_scenario(cf_client):
    """
    Verify systems show as 'behind' after rolling back to older commit.

    Setup: System deployed with newer commit, then rolled back
    Expected: deployment_status='behind', commits_behind=1
    """
```

## Performance Testing

### Benchmarks

- View query execution: < 10 seconds
- Agent heartbeat processing: < 100ms
- Webhook processing: < 5 seconds
- Build evaluation trigger: < 30 seconds

### Load Testing

```python
def test_concurrent_heartbeats(server_vm, num_agents=100):
    """Test server handles concurrent agent heartbeats"""
    # Implementation using pytest-xdist for parallel execution
```

## Security Testing

### Areas of Focus

- Ed25519 signature validation
- SQL injection prevention
- API authentication/authorization
- Network isolation in VMs
- Privilege escalation prevention

### Security Test Examples

```python
def test_unsigned_heartbeat_rejected(server_vm, agent_vm):
    """Verify server rejects heartbeats without valid signatures"""

def test_sql_injection_prevention(cf_client):
    """Verify views are safe from SQL injection"""
```

## Test Output & Reporting

### Test Results Location

- **HTML Reports**: `test-results/report.html`
- **Coverage Reports**: `test-results/coverage/`
- **VM Test Logs**: `.nixos-test-history`

## Test Maintenance

### Regular Tasks

- **Weekly**: Review and update failing tests
- **Monthly**: Audit test coverage metrics
- **Quarterly**: Scenario data refresh
- **Per release**: Full regression suite

### Test Debt Management

- Track flaky tests in issues
- Prioritize test stability over new tests
- Regular test refactoring sprints

## Development Workflow

### Adding New Tests

1. **Database View Tests**: Add to `packages/cf-test-modules/cf_test/tests/test_view_<name>.py`
2. **Scenarios**: Extend `packages/cf-test-modules/cf_test/scenarios/`
3. **Integration Tests**: Create `packages/cf-test-modules/cf_test/tests/test_integration_<feature>.py`
4. **Unit Tests**: Add to relevant Rust modules with `#[test]`

### Running Tests During Development

```bash
# Quick feedback loop
nix develop
server-stack up
run-db-test -vvv -m database

```

## Questions for Clarification

Before finalizing this test plan, please clarify:

1. **Performance baselines**: What are acceptable response times for critical operations?
2. **Failure scenarios**: Which failure modes are most critical to test (network partitions, database failures, etc.)?
3. **Compliance frameworks**: Which specific compliance checks (STIG, NIST) need dedicated test scenarios?
4. **Scale testing**: What's the expected maximum fleet size we should test?
5. **CVE scanning**: Should we test with real CVE data or synthetic vulnerabilities?
6. **Deployment testing**: Do we need tests for agent deployment/updates?
7. **Monitoring integration**: Should we test Grafana dashboard queries?

## Success Metrics

- **Test reliability**: < 1% flaky test rate
- **Execution speed**: Full suite < 10 minutes
- **Coverage growth**: +5% coverage per sprint
- **Bug detection**: > 80% bugs caught in testing
- **Documentation**: 100% of tests documented

## Next Steps

1. Implement missing scenario builders for complex fleet behaviors
2. Add performance benchmarking framework
3. Create security-focused test scenarios
4. Develop load testing harness
5. Set up continuous coverage reporting
6. Document test patterns for common scenarios
7. Create test data fixtures for reproducible testing
