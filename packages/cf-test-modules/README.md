# VM Test Logger

A comprehensive logging framework for NixOS VM tests that provides structured, readable test output with automatic artifact collection and error handling.

## Overview

The VM Test Logger simplifies NixOS VM testing by providing:

- **Structured logging** with timestamps and log levels
- **Automatic artifact collection** from multiple VMs
- **Service log capture** with error handling
- **Network connectivity testing** utilities
- **Database query execution** and validation
- **Reusable test patterns** for common scenarios
- **Robust error handling** to prevent test failures from missing logs

## Installation

Add the VM Test Logger to your NixOS test's `extraPythonPackages`:

```nix
{
  testScript = ''
    # Your test script here
  '';

  extraPythonPackages = p: [
    pkgs.crystal-forge.vm-test-logger
  ];
}
```

## Quick Start

### Basic Usage

```python
from vm_test_logger import TestLogger, TestPatterns

# Initialize logger with test name and primary VM
logger = TestLogger("My Integration Test", server)

# Start all VMs and setup logging
start_all()
logger.setup_logging()

# Wait for services to start
TestPatterns.standard_service_startup(logger, server, [
    "postgresql",
    "my-service.service",
])

# Capture service logs
logger.capture_service_logs(server, "my-service.service")

# Test network connectivity
TestPatterns.network_test(logger, server, "server", 3000)

# Finalize and collect all artifacts
logger.finalize_test()
```

### Using the Decorator

```python
from vm_test_logger import with_logging

@with_logging("My Test", primary_vm_name="server")
def test_my_service(logger):
    # Logger is automatically injected
    logger.log_success("Test started")

    # Your test logic here

    # Cleanup happens automatically
```

## Core Classes

### TestLogger

The main logging class that provides all logging and artifact collection functionality.

#### Constructor

```python
TestLogger(test_name: str, primary_vm: Any, start_time: float = None, log_files: List[str] = None)
```

- **test_name**: Human-readable name for the test
- **primary_vm**: The primary VM instance (usually the server)
- **start_time**: Test start time (auto-generated if not provided)
- **log_files**: List to track generated log files (auto-initialized)

#### Key Methods

##### Logging Methods

```python
# Basic logging with different levels
logger.log("General message", "INFO")
logger.log_success("Operation completed")
logger.log_info("Additional details")
logger.log_error("Something went wrong")
logger.log_warning("Potential issue")
logger.log_section("🚀 Starting new test phase")
```

##### Service Management

```python
# Wait for services to be ready
logger.wait_for_services(vm, ["postgresql", "nginx"])

# Capture service logs with automatic error handling
logger.capture_service_logs(vm, "my-service.service")
logger.capture_service_logs(vm, "my-service.service", "custom-filename.log")
```

##### Command Execution & Capture

```python
# Execute command and capture output to file
logger.capture_command_output(
    vm,
    "systemctl status my-service",
    "service-status.txt",
    "Service Status Check"
)

# Database queries with result capture
result = logger.database_query(
    vm,
    "mydb",
    "SELECT * FROM users WHERE active = true",
    "active-users.txt"
)
```

##### File & Network Testing

```python
# Verify files exist and are readable
logger.verify_files(vm, {
    "/etc/my-config.conf": "Configuration file accessible",
    "/var/lib/my-service/data": "Data directory accessible"
})

# Test network connectivity
logger.test_network_connectivity(source_vm, "target-host", 8080)
```

##### System Information

```python
# Gather system details
info = logger.gather_system_info(vm)
# Returns: {"hostname": "...", "system_hash": "...", "uptime": "..."}
```

##### Assertions & Validation

```python
# Assert content exists in output with proper logging
logger.assert_in_output(
    "Service active",
    service_output,
    "Service activation check"
)
```

### TestPatterns

Static utility class providing common test patterns.

#### Service Patterns

```python
# Standard service startup sequence
TestPatterns.standard_service_startup(logger, vm, [
    "postgresql",
    "redis",
    "my-application.service"
])

# Multi-VM service log collection
TestPatterns.capture_service_logs_multi_vm(logger, [
    (server, "my-server.service"),
    (agent, "my-agent.service"),
    (database, "postgresql.service")
])
```

#### File & Network Patterns

```python
# Key file verification
TestPatterns.key_file_verification(logger, vm, {
    "/etc/ssl/certs/my-cert.pem": "SSL certificate accessible",
    "/etc/my-app/config.json": "Application config accessible"
})

# Network connectivity testing
TestPatterns.network_test(logger, client_vm, "server", 3000)
```

#### Database Patterns

```python
# Database content validation
TestPatterns.database_verification(logger, vm, "mydb", {
    "user_count": "5",
    "status": "active",
    "version": "1.0.0"
})
```

## Advanced Usage

### Multi-VM Testing

```python
# Initialize logger with primary VM
logger = TestLogger("Multi-VM Integration Test", server)
logger.setup_logging()

# Test services on different VMs
TestPatterns.standard_service_startup(logger, server, ["postgresql"])
TestPatterns.standard_service_startup(logger, agent, ["my-agent.service"])
TestPatterns.standard_service_startup(logger, worker, ["my-worker.service"])

# Collect logs from all VMs
TestPatterns.capture_service_logs_multi_vm(logger, [
    (server, "postgresql.service"),
    (agent, "my-agent.service"),
    (worker, "my-worker.service")
])

# Test inter-VM communication
logger.test_network_connectivity(agent, "server", 5432)
logger.test_network_connectivity(worker, "server", 5432)
```

### Custom Log Capture

```python
# Capture custom command output
logger.capture_command_output(
    vm,
    "journalctl --since='1 hour ago' | grep ERROR",
    "recent-errors.txt",
    "Recent Error Analysis"
)

# Capture system diagnostics
logger.capture_command_output(
    vm,
    "df -h && free -h && ps aux --sort=-%cpu | head -10",
    "system-health.txt",
    "System Health Check"
)
```

### Database Testing

```python
# Test database connectivity and content
logger.log_section("🗄️ Database Validation")

# Basic connectivity
logger.database_query(vm, "myapp", "SELECT version();", "db-version.txt")

# Data validation
result = logger.database_query(
    vm,
    "myapp",
    "SELECT COUNT(*) as user_count FROM users WHERE created_at > NOW() - INTERVAL '1 day';"
)

# Assert expected data exists
logger.assert_in_output("user_count", result, "New users created today")
```

### Error Handling & Recovery

```python
# The logger handles missing files gracefully
try:
    logger.capture_service_logs(vm, "optional-service.service")
except:
    logger.log_warning("Optional service not running - continuing test")

# Verify critical vs optional components
logger.verify_files(vm, {
    "/etc/critical-config.conf": "Critical configuration (required)"
})

# Optional file check with custom handling
try:
    vm.succeed("test -f /etc/optional-config.conf")
    logger.log_success("Optional configuration found")
except:
    logger.log_info("Optional configuration not present (expected)")
```

## Log Output Format

The logger produces structured, timestamped output:

```
🚀 Starting My Integration Test
============================================================
Test started at: 2025-08-17 14:30:25 UTC
✅ All VMs started successfully

[14:30:26] INFO:
⏳ Waiting for essential services to start...
[14:30:26] INFO:   • postgresql...
[14:30:28] SUCCESS: ✅ postgresql is ready
[14:30:28] INFO:   • my-service.service...
[14:30:30] SUCCESS: ✅ my-service.service is ready

[14:30:30] INFO:
📄 Capturing my-service.service logs...
[14:30:31] SUCCESS: ✅ Service logs captured: my-service-logs.txt

[14:30:31] INFO:
🌐 Testing network connectivity...
[14:30:32] SUCCESS: ✅ Server listening on port 3000
[14:30:32] SUCCESS: ✅ Can reach server

[14:30:35] INFO:
🎉 Test completed successfully at: 2025-08-17 14:30:35 UTC
⏱️  Test duration: 9.42 seconds
============================================================
✅ My Integration Test PASSED

📁 Generated log files:
[14:30:35] INFO:   • test-results.log
[14:30:35] INFO:   • my-service-logs.txt
[14:30:35] INFO:   • system-health.txt
```

## Generated Artifacts

The logger automatically generates and collects these artifacts:

### Core Artifacts

- **test-results.log**: Complete test execution log with timestamps
- **{service}-logs.txt**: Individual service logs from journalctl
- **database-query.txt**: Database query results
- **system-health.txt**: System diagnostic information

### Custom Artifacts

- Any files created via `capture_command_output()`
- Service-specific log files
- Application-generated logs and reports

All artifacts are automatically copied from VMs to the host system at test completion.

## Error Handling

The logger includes robust error handling for common test scenarios:

### Missing Services

```python
# Gracefully handles services that don't exist
logger.capture_service_logs(vm, "non-existent.service")
# Creates placeholder file instead of failing
```

### Missing Files

```python
# Won't fail if log files can't be copied
logger.finalize_test()  # Logs warnings but continues
```

### Network Issues

```python
# Provides clear error messages for connectivity problems
logger.test_network_connectivity(vm, "unreachable-host", 8080)
```

### Command Failures

```python
# Captures command failures instead of stopping tests
logger.capture_command_output(vm, "failing-command", "output.txt")
# File will contain error message instead of breaking test
```

## Best Practices

### Test Structure

```python
def test_my_service():
    logger = TestLogger("Service Integration Test", server)

    # 1. Setup phase
    start_all()
    logger.setup_logging()

    # 2. Service startup
    TestPatterns.standard_service_startup(logger, server, ["postgresql"])

    # 3. Configuration verification
    TestPatterns.key_file_verification(logger, server, {
        "/etc/myapp/config.json": "Application configuration"
    })

    # 4. Functional testing
    logger.test_network_connectivity(server, "server", 8080)

    # 5. Data validation
    TestPatterns.database_verification(logger, server, "myapp", {
        "status": "ready"
    })

    # 6. Cleanup
    logger.finalize_test()
```

### Logging Guidelines

1. **Use descriptive section headers**: `logger.log_section("🔧 Configuration Setup")`
2. **Log progress frequently**: Help debug long-running operations
3. **Capture relevant artifacts**: Service logs, command outputs, system state
4. **Use appropriate log levels**: Success/Info/Warning/Error
5. **Include context in assertions**: Descriptive messages for validation failures

### Performance Considerations

1. **Batch service log collection**: Use `capture_service_logs_multi_vm()` for multiple VMs
2. **Limit log capture scope**: Use specific time ranges for journalctl
3. **Clean up temporary files**: Logger handles this automatically
4. **Use timeouts appropriately**: For network and service operations
