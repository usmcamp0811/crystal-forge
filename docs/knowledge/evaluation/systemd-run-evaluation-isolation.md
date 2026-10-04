---
type: Concept
title: "SystemD-Run Evaluation Isolation in Crystal Forge"
description: "Explains the systemd-run scope isolation (memory, CPU, timeout limits) and the direct-execution fallback that keep Nix evaluations from OOM-killing the server, with trade-offs, failure modes, and proposed improvements."
tags:
  - crystal-forge
  - evaluation
  - systemd-run
  - oom
  - resource-limits
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:55:03-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/dev-notes.md at commit 3b23d36f"
    title: "Developer notes (dev-notes.md)"
---
# SystemD-Run Evaluation Isolation in Crystal Forge

> **Status:** The document shows fixed values `MemoryMax=4G`, `CPUQuota=300%`, and `TimeoutStopSec=600` and a `--user` scope. In the code at the migration base these limits are optional `BuildConfig` fields (`systemd_memory_max`, `systemd_cpu_quota`, `systemd_timeout_stop_sec`, `systemd_properties`) in `packages/default/crates/cf-config/src/config/build.rs`, applied in `packages/default/crates/cf-server/src/derivations/utils.rs` and `packages/default/crates/cf-builder/src/derivations/utils.rs`. A search of `packages/default/crates` did not find the helper names `run_dry_run` or `run_direct_dry_run` or the fallback strings quoted below. A later verification pass must compare the whole document with those files.

## Background

Crystal Forge was experiencing OOM (Out of Memory) killer terminations during Nix evaluations, causing the entire server process to be killed when individual derivation evaluations consumed excessive system resources. This document explains the isolation strategy implemented to prevent these failures.

## The Problem

### OOM Killer Behavior

When Crystal Forge performed large Nix evaluations:

- Individual evaluations could consume 8GB+ of memory
- Memory usage would spike rapidly during derivation resolution
- Linux OOM killer would terminate the entire Crystal Forge server process
- This caused loss of all in-progress work and service downtime
- Recovery required manual service restart

### Previous Architecture Limitations

```rust
// Original approach - no isolation
let mut cmd = Command::new("nix");
cmd.args(["build", flake_target, "--dry-run"]);
let output = cmd.output().await?; // Could consume unlimited memory
```

This approach provided no protection against runaway evaluations consuming system resources.

## The Solution: SystemD-Run Scope Isolation

### Implementation Strategy

We implemented a fallback-based approach using `systemd-run` for resource isolation:

```rust
fn systemd_scoped_cmd_base() -> Command {
    let mut c = Command::new("systemd-run");
    c.args([
        "--user",                           // Run in user scope
        "--scope",                          // Create transient scope
        "--collect",                        // Auto-cleanup on exit
        "--property=MemoryMax=4G",          // Hard memory limit
        "--property=CPUQuota=300%",         // CPU limit (3 cores)
        "--property=TimeoutStopSec=600",    // 10 minute timeout
        "--quiet",                          // Reduce log noise
        "--",
        "nix", "build",
    ]);
    c
}
```

### Fallback Architecture

The implementation includes automatic fallback for environments where `systemd-run` is unavailable:

1. **Primary**: Try systemd-scoped execution with resource limits
2. **Fallback Detection**: Check for specific systemd/D-Bus failures
3. **Secondary**: Fall back to direct `nix build` execution
4. **Error Handling**: Distinguish between resource failures and evaluation failures

## Technical Implementation

### Three-Tier Execution Strategy

#### Tier 1: Systemd Scoped Evaluation

```rust
async fn run_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
    let mut scoped = Self::systemd_scoped_cmd_base();
    scoped.args([flake_target, "--dry-run", "--print-out-paths", "--no-link"]);

    match scoped.output().await {
        Ok(output) if output.status.success() => {
            info!("✅ Systemd-scoped evaluation completed successfully");
            Ok(output)
        }
        // ... fallback logic
    }
}
```

#### Tier 2: Fallback Detection

```rust
// Check for systemd-specific issues that indicate fallback needed
if stderr.contains("Failed to connect to user scope bus")
    || stderr.contains("DBUS_SESSION_BUS_ADDRESS")
    || stderr.contains("XDG_RUNTIME_DIR")
    || stderr.contains("Failed to create scope")
    || stderr.contains("systemd")
{
    warn!("⚠️ Systemd scope creation failed (no user session), falling back to direct execution");
    Self::run_direct_dry_run(flake_target, build_config).await
}
```

#### Tier 3: Direct Execution

```rust
async fn run_direct_dry_run(flake_target: &str, build_config: &BuildConfig) -> Result<Output> {
    let mut direct = Command::new("nix");
    direct.args(["build", flake_target, "--dry-run", "--print-out-paths", "--no-link"]);
    build_config.apply_to_command(&mut direct);
    Ok(direct.output().await?)
}
```

### Resource Limit Configuration

#### Memory Management

- **MemoryMax=4G**: Hard limit prevents individual evaluations from consuming excessive RAM
- **Scope isolation**: OOM kills affect only the evaluation scope, not the main server
- **Cleanup**: Systemd automatically cleans up terminated scopes

#### CPU Management

- **CPUQuota=300%**: Limits to 3 CPU cores maximum
- **Prevents**: CPU starvation of other system processes
- **Balances**: Evaluation speed vs system responsiveness

#### Timeout Protection

- **TimeoutStopSec=600**: 10-minute evaluation timeout
- **Prevents**: Hung evaluations from consuming resources indefinitely
- **Graceful**: Allows time for complex evaluations while preventing deadlock

## Fallback Scenarios

### When Fallback Occurs

1. **Missing SystemD**: `systemd-run` command not available
2. **No User Session**: D-Bus session bus unavailable
3. **Permission Issues**: Insufficient privileges for scope creation
4. **Runtime Environment**: Container or restricted environments

### Fallback Detection Logic

The system identifies fallback scenarios through:

- **Command availability**: `std::io::ErrorKind::NotFound`
- **Error message patterns**: D-Bus and systemd-specific error text
- **Graceful degradation**: Automatic retry with direct execution

### Error Categorization

```rust
// Environment issues - fallback
if stderr.contains("Failed to connect to user scope bus") { /* fallback */ }

// Actual evaluation failures - propagate
else { /* return original error */ }
```

## Benefits and Trade-offs

### Benefits

- **Server Stability**: OOM killer can no longer terminate main server process
- **Resource Protection**: System remains responsive during large evaluations
- **Automatic Recovery**: Failed evaluations don't require manual intervention
- **Graceful Degradation**: Works in environments without full systemd support
- **Observable Isolation**: Clear logging shows when isolation is active

### Trade-offs

- **Complexity**: More complex execution path with fallback logic
- **Dependencies**: Relies on systemd user session functionality
- **Resource Overhead**: Slight overhead from scope creation/cleanup
- **Debugging**: Multiple execution paths can complicate troubleshooting

### Performance Impact

- **Scope Creation**: ~10-50ms overhead per evaluation
- **Memory Efficiency**: Better overall memory usage due to isolation
- **CPU Fairness**: More predictable CPU allocation
- **I/O Impact**: Minimal impact on disk/network operations

## Operational Considerations

### Monitoring Points

- **Scope Success Rate**: Percentage of evaluations using systemd scopes
- **Fallback Frequency**: How often direct execution is used
- **Resource Violations**: Evaluations hitting memory/CPU limits
- **Timeout Events**: Evaluations exceeding time limits

### Logging Strategy

```rust
info!("✅ Systemd-scoped evaluation completed successfully");
warn!("⚠️ Systemd scope creation failed (no user session), falling back to direct execution");
info!("📋 systemd-run not available, using direct execution");
```

### Failure Modes

1. **Scope Creation Failure**: Automatic fallback to direct execution
2. **Resource Limit Hit**: Evaluation terminated, scope cleaned up automatically
3. **Timeout**: Graceful termination after 10 minutes
4. **System Resource Exhaustion**: Only affects individual scope, not server

## Architecture Evolution

### Before: Monolithic Evaluation

```
Crystal Forge Server Process
├── All evaluations run in-process
├── Memory consumption unlimited
├── OOM kill affects entire server
└── No resource isolation
```

### After: Isolated Evaluation

```
Crystal Forge Server Process
├── Systemd User Session
│   ├── Evaluation Scope 1 (4GB limit)
│   ├── Evaluation Scope 2 (4GB limit)
│   └── Evaluation Scope N (4GB limit)
├── Fallback to direct execution
└── Server process protected from evaluation OOM
```

## Future Improvements

> **Status:** proposed. The items below are future improvements in the source; this migration did not verify any of them.

### Enhanced Resource Management

- Dynamic memory limits based on system capacity
- Priority-based CPU allocation for different evaluation types
- Disk I/O limits for large derivation builds

### Evaluation Caching

- Cache successful evaluations to avoid re-computation
- Invalidation based on flake input changes
- Shared cache across multiple Crystal Forge instances

### Advanced Isolation

- Container-based isolation for complete environment separation
- Network namespace isolation for security
- Filesystem isolation for build cleanliness

This isolation strategy successfully eliminated OOM-related server terminations while maintaining compatibility across different deployment environments through intelligent fallback mechanisms.
