---
type: Operator Guide
title: Bulk evaluator memory planning and timeouts
description: Explains automatic and explicit memory sizing, effective worker resolution, independent deadlines, upgrade behavior, and version-dependent nix-eval-jobs thresholds for server bulk commit evaluation.
tags:
  - crystal-forge
  - evaluation
  - memory
  - configuration
implementation_status: implemented
generated:
  by: opencode/gpt-6.1-sol
  at: 2026-10-09T09:08:39-05:00
---

# Bulk evaluator memory planning and timeouts

## Scope

This guide describes the server's bulk commit evaluator resource contract.
The configuration fields, generated TOML, and evaluator call site use
`eval_output_idle_timeout_secs` and `eval_overall_timeout_secs`. The resolved
resource plan supplies `per_worker_mb` to the evaluator command.

The implementation status records source behavior. It does not establish a
passing runtime, packaging, or OKF validation result.

These controls apply to the authoritative `nix-eval-jobs` invocation. They do
not change hardening scans, Config Inspector subprocesses, builder evaluation
or realization, or systemd cgroup configuration. Resource settings can stop an
evaluation; they do not change a successful `.drvPath` or the
[verified-source evaluator fingerprint](../builders/verified-source-evaluator-contract.md).

## Configuration contract

All keys below are in TOML `[server]` or NixOS
`services.crystal-forge.server`.

| Key | Default | Meaning |
| --- | --- | --- |
| `eval_workers` | `2` | Parallel evaluator workers. `0` selects automatic worker resolution. |
| `eval_max_memory_mb` | absent in TOML; `null` in Nix | Optional positive integer MiB per worker. Absence selects automatic memory sizing. |
| `eval_memory_reserve_mb` | `4096` | Positive integer MiB reserved from the effective memory limit during automatic sizing. |
| `eval_memory_max_percent` | `85` | Percentage of the effective memory limit available to automatic sizing; integer range `1..100`. |
| `eval_output_idle_timeout_secs` | `900` | Maximum interval without evaluator output, in seconds. Must be positive. |
| `eval_overall_timeout_secs` | `3600` | Independent elapsed-time deadline for the evaluator, in seconds. Must be positive. |

The compatibility name `_mb` denotes MiB, not decimal megabytes. An explicit
`eval_max_memory_mb = 12288` still passes `--max-memory-size 12288` for each
worker. With two workers, the configured product remains 24576 MiB. Automatic
reserve and percentage settings do not alter that explicit value.
If the fixed configured product exceeds the detected working boundary, the
server emits a static warning without changing the normal child threshold.
An isolated recovery child has a separate cap derived from the original plan;
see [Adaptive recovery](bulk-evaluator-adaptive-recovery.md).

The Nix option is `nullOr` positive integer, with a `null` default. The config
generator omits a null value from TOML. TOML has no null literal: remove the
`eval_max_memory_mb` key to enable automatic sizing. Do not write
`eval_max_memory_mb = null` or a quoted `"null"`.

## Effective workers and immutable resource plan

For `eval_workers = 0`, Crystal Forge calls Rust
`std::thread::available_parallelism()` and resolves a positive worker count.
It passes that resolved count explicitly as `--workers <count>` rather than
passing zero and letting upstream select another value. Detection failure is
a clear pre-spawn error. An explicit nonzero worker count remains unchanged.

The evaluator acquires the existing heavy-Nix locks in their established
order: PostgreSQL advisory lock, then the in-process permit. After acquiring
both locks and before spawning, it resolves one immutable resource plan.
Normal children use that plan's worker count and memory threshold. Recovery
derives its isolated cap from the same snapshot; it does not resample capacity
or resize in response to free memory, RSS, or `memory.current` changes. One
overall deadline remains fixed for the invocation and all replacement children.

## Automatic memory sizing

Crystal Forge detects stable memory limits at runtime:

1. Physical memory from Linux `MemTotal`.
2. The minimum finite cgroup-v2 `memory.high` and `memory.max` boundaries for
   the actual server process and its visible ancestors, resolved independently
   using its cgroup membership and mount layout. The detector selects the
   broadest matching visible cgroup-v2 mount
   and walks from the process cgroup toward that mount's root. It includes the
   root limits when present. Some kernels have no memory-controller files at
   the hierarchy root. An unlimited child `max` does not hide a finite
   ancestor service limit.
   Ancestors hidden by a cgroup namespace cannot be observed.

For each boundary, an ancestor read or parse failure leaves detected finite
limits intact and emits a static warning. Failure to read one boundary does
not discard the other. The detector continues toward the visible mount root;
if no finite limit remains for the failed boundary, that boundary is unknown.
A missing controller file is allowed only at the verified hierarchy root.
Missing non-root files are detection failures. An incomplete snapshot cannot
account for an unreadable stricter ancestor boundary. Hierarchy resolution is
bounded to 32 validated directories, with the actual cgroup first and root last.

The effective working boundary is the minimum detected physical memory,
finite `memory.high`, and finite `memory.max`. Unknown boundaries do not
contribute a value. `max` means unlimited; numeric `0` is finite, not unknown.
A zero working boundary rejects automatic evaluation before spawn.
`memory.high` is a reclaim/throttling threshold, not an OOM-kill limit. Including
it avoids sizing only against a larger hard limit while reclaim already stalls
the service. `memory.max` supplies hard cgroup containment when configured.
These settings do not install or change either controller value.

Do not use `MemAvailable`, current free memory, process RSS, or cgroup
`memory.current` as the sizing input. Those
measure current load, not a stable resource allowance.

Let `M` be the effective working boundary in MiB, `R` the reserve, `P` the
percentage, and `W` the resolved worker count. The plan computes:

```text
percentage_target = floor(M * P / 100)
reserve_target = M - R
aggregate_target = min(percentage_target, reserve_target)
per_worker = floor(aggregate_target / W)
configured_product = W * per_worker
```

Arithmetic is checked. A detected limit that cannot accommodate the reserve
or produce a positive threshold per worker is an error before spawn, not a
reason to use the fallback. Zero explicit thresholds, invalid percentages,
and overflow are rejected. There is no fixed 32768 MiB / 32 GiB ceiling and
no requirement to enter the host's RAM manually.

Only if no capacity source provides a usable boundary does automatic sizing
fall back to 4096 MiB per effective worker. Detection and fallback warnings use
static, credential-safe messages; they do not include file contents or raw
detector errors. An explicit override bypasses automatic memory sizing.

### Worked example

| Input or result | MiB unless specified |
| --- | --- |
| Physical `MemTotal` | 65536 |
| Actual process cgroup `memory.max` | 49152 |
| Visible `memory.high` boundaries | unlimited |
| Effective limit `M` | 49152 |
| Reserve `R` | 4096 |
| Percentage `P` | 85 percent |
| Percentage target `floor(49152 * 85 / 100)` | 41779 |
| Reserve target `49152 - 4096` | 45056 |
| Aggregate target | 41779 |
| Effective workers `W` | 2 workers |
| Per-worker threshold | 20889 |
| Configured product | 41778 |
| Unassigned rounding remainder | 1 |

The command receives `--workers 2 --max-memory-size 20889`.

If the same illustrative host instead has a finite ancestor `memory.high` of
39321 MiB, the working boundary is 39321 MiB. The percentage target is 33422
MiB and the reserve target is 35225 MiB. Two workers receive 16711 MiB each.
This is a calculation example, not a measured deployment result.

## Threshold policy versus hard containment

The aggregate target is a Crystal Forge sizing policy. It is not a hard
guarantee that the evaluator subtree will stay below that memory amount.
Collector overhead and in-flight worker growth can exceed the product.
Hard service containment belongs to the server's systemd cgroup limits.
Changing these evaluator settings does not add or change a cgroup limit.

The repository packages exactly `nix-eval-jobs` **2.34.3**, linked against
Nix **2.34.8**. Those are different version numbers for different components.
In upstream [v2.34.3 `src/worker.cc`](https://github.com/NixOS/nix-eval-jobs/blob/v2.34.3/src/worker.cc),
`shouldRestart` compares the worker's own `getrusage(RUSAGE_SELF).ru_maxrss`
peak RSS to `--max-memory-size` after the job response. An oversized worker
then requests a restart. This version does not implement aggregate-worker
memory killing or a retry-alone policy. A single job can exceed the threshold
before the check runs.

Upstream [v2.35.4 scheduler source](https://github.com/NixOS/nix-eval-jobs/blob/v2.35.4/src/nix-eval-jobs.cc)
uses an aggregate `workers * max-memory-size` budget, samples RSS at a
200-millisecond cadence, and can kill a busy worker and retry its job alone.
Its terminal solo diagnostic is free-form error text, not a dedicated resource
error code. The repository still packages v2.34.3. Confirm the deployed binary
and source before attributing v2.35.4 behavior to a deployment. The automatic
plan supplies a derived threshold in either case; it does not turn an upstream
threshold into hard containment. See
[runtime packaging](../../../checks/builder-evaluator-packaging/README.md)
for exact evaluator/Nix selection. This resource policy does not change the
flake pin or the guarded netrc patch.

## Independent deadlines and cancellation

The defaults remain 900 seconds for output-idle assessment and 3600 seconds
for the independent overall deadline. The overall deadline starts once before
the initial child and covers normal evaluation, recovery, small fallback,
build-preparation drain, and foreground cleanup. Replacement children do not
reset that deadline. Each child's idle clock starts at its spawn.
A complete nonblank line received on evaluator stdout or stderr before expiry
advances the idle clock; it never extends the overall deadline. Blank lines
do not advance activity. Late buffered output
cannot revive an expired deadline. Server-generated progress messages and
cancellation polls are not evaluator output and do not extend either deadline.
The overall deadline limits elapsed execution time, not CPU time, and is not
a security sandbox boundary.

An outer monitor assesses idle expiry and enforces the overall deadline across
every await in evaluator collection, including output-handler database work,
log flushing, and the final child wait after both pipes reach EOF. A bounded
Tokio watch channel shares
only the latest monotonic output timestamp with the monitor; it does not queue
output or allocate per line. Output activity wakes the monitor even while
collection is inside a handler await. Overall expiry takes precedence over a
ready collection result. Idle expiry is assessed without extending that ceiling.

Idle expiry is an assessment checkpoint. Complete, fresh samples with CPU
progress by a direct evaluator worker and no `D`-state member can waive silence
until the next sample. This does not manufacture output or extend the overall
deadline. Unknown or incomplete samples cannot establish healthy work. See
[Adaptive recovery](bulk-evaluator-adaptive-recovery.md) for the exact evidence
requirements and bounded isolation policy. Silence alone is not OOM evidence.

Cancellation polls have a two-second cooperative cadence at collection-loop
boundaries. This is not a guarantee that a cancellation request is detected
within two seconds. An output-handler or database await can delay the next
cancellation query. Idle assessment, pressure sampling, and overall monitoring
remain active during those awaits independently of cancellation polling.

Collection does not terminate or disarm the process guard. On a phase stop,
the monitor drops collection before foreground cleanup confirms direct-child
reap and process-group absence. Cleanup runs outside the collection race but
uses the invocation's remaining deadline. Optional timeout-log persistence
must follow cleanup. An unconfirmed group transfers cleanup ownership and
heavy-Nix locks to the runtime reaper; it does not authorize a replacement.
Quarantine can retain those locks after the monotonic invocation deadline.
That deadline bounds foreground waiting, not guaranteed exit of a pending
`D`-state member.
Output EOF, a signal request, or a diagnostic process scan is not cleanup
acknowledgement. This protects inherited process groups, not processes that
escape the group. See the recovery guide for quarantine behavior.

## Upgrade and operator examples

An upgrade with no explicit memory override enables automatic sizing. It also
uses the new 900-second idle and 3600-second overall defaults when those keys
are absent. Review the loaded configuration and resolved resource plan when
upgrading; the old absent-key behavior was fixed 4096 MiB per worker.

Automatic NixOS configuration:

```nix
services.crystal-forge.server = {
  eval_workers = 2;
  eval_max_memory_mb = null; # Omitted from generated TOML.
  eval_memory_reserve_mb = 4096;
  eval_memory_max_percent = 85;
  eval_output_idle_timeout_secs = 900;
  eval_overall_timeout_secs = 3600;
};
```

Equivalent TOML:

```toml
[server]
eval_workers = 2
# No eval_max_memory_mb key: automatic sizing.
eval_memory_reserve_mb = 4096
eval_memory_max_percent = 85
eval_output_idle_timeout_secs = 900
eval_overall_timeout_secs = 3600
```

To retain a fixed threshold, set `eval_max_memory_mb = 4096` explicitly.
To select 12 GiB per worker, set `eval_max_memory_mb = 12288` in Nix or TOML.
With `eval_workers = 2`, that second configuration preserves a 24576 MiB
configured product regardless of detected RAM.

For an idle checkpoint, inspect whether output stopped and whether fresh
direct-worker CPU samples justify continued collection.
For an overall timeout, inspect elapsed evaluation duration even if output
continued. For memory failures, distinguish a threshold-triggered upstream
restart from a service-cgroup OOM. Avoid increasing all controls together:
each setting has a separate failure condition.
