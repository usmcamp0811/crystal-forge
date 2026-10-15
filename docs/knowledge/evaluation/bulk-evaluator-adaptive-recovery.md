---
type: Operator Guide
title: Bulk evaluator adaptive recovery and partial resource failure
description: Explains pressure evidence, bounded isolated recovery, shared deadlines, cleanup quarantine, and exact-attempt retention when bulk evaluation cannot finish within its resource budget.
tags:
  - crystal-forge
  - evaluation
  - recovery
  - memory
  - operations
implementation_status: implemented
---

# Bulk evaluator adaptive recovery and partial resource failure

## Scope and source status

Adaptive recovery belongs to the server's authoritative bulk commit evaluator.
It uses the existing verified source, revision, assigned policies, result
parser, Config-root capture, and snapshot paths. It does not change builder
fingerprints, pure evaluation, IFD, source identity, or cache authorization.
See [Memory planning and timeouts](bulk-evaluator-resource-planning.md) for
configuration defaults and the immutable capacity plan.

The runtime recovery controller, pressure sampler, cleanup quarantine, and
partial-failure finalizer use a shared durable reconciliation proof. Runtime
stages unacknowledged persistence candidates separately from completed
outcomes and passes unresolved candidates to the lifecycle finalizer.
Implementation status describes source behavior, not passing PostgreSQL,
runtime, or final verification gates. No production-host pressure measurement
or execution of a newer upstream evaluator is claimed here.

## Working boundary and recovery budget

The capacity plan uses the minimum detected physical `MemTotal`, finite
visible-ancestor `memory.high`, and finite visible-ancestor `memory.max`.
`memory.high` starts reclaim/throttling; it is not an OOM-kill limit.
`memory.max` is the hard cgroup limit. The detector treats numeric zero as
finite and `max` as unlimited. It reads high and max independently so one
failed read cannot discard the other boundary.

The normal child preserves an explicit per-worker override exactly. For
example, two workers with `eval_max_memory_mb = 12288` retain their 24576 MiB
configured product. A product above the detected working boundary emits a
static warning. Automatic sizing retains the 4096 MiB reserve and 85-percent
defaults described in the memory guide.

An isolated child uses one worker and a separate threshold from the original
capacity snapshot:

```text
working_budget = min(floor(working_boundary * percent / 100),
                     working_boundary - reserve)
solo_threshold = min(original_total_budget, working_budget)
```

If no working boundary was detected, the isolated threshold is the smaller
of the original total budget and 4096 MiB. No positive safe isolated budget
means terminal resource failure. Live `memory.current`, free memory, and RSS
never resize the plan. Reading those values for diagnosis is separate from
capacity sizing.

## Private pressure observations

The owner samples every 30 seconds without spawning a diagnostic task or
queuing samples. Process identity is `(PID, start_ticks)`; PID reuse cannot
produce a false CPU delta. Observations include process-group membership,
parent PID, recognized state, RSS, CPU deltas, worker replacement, cgroup
usage, memory-event deltas, and memory/I/O pressure-stall information (PSI).

| Bound | Source behavior |
| --- | --- |
| Numeric procfs entries scanned | At most 4096 |
| Process-group members retained | At most 256 |
| Detailed process log entries | At most 16 per sample |
| Visible cgroup directories | At most 32, actual cgroup first and verified root last |
| Text reads | At most 4096 bytes per file |
| Scalar reads | At most 128 bytes per file |
| Cooperative collection budget | 250 milliseconds, checked between filesystem calls |

The 250-millisecond budget bounds cooperative work and allocation, not the
latency of a blocking kernel filesystem call. It is not a hard real-time
deadline. Truncation, parse failure, missing data, and budget exhaustion make
the sample incomplete. First observations and counter resets lack delta
baselines. Unknown evidence is neither healthy work nor an attributed OOM.

An existing final hierarchy root with no memory-controller files is skipped.
It does not invalidate complete child observations. A root-only hierarchy
provides no cgroup pressure evidence. Missing non-root files, a vanished
root, partial controller files, and permission failures remain unknown;
they must not be treated as healthy pressure clearance.

Pressure logs contain bounded numeric fields, recognized states, and finite
reason labels. They do not contain process command lines, arguments,
environment variables, Nix expressions, or raw procfs/cgroup contents.
Configuration labels and persisted resource summaries use the existing
redaction path and bounded fields. The terminal summary includes at most four
remaining names and four resource entries, with fields clipped to 32 characters
and an explicit truncation indicator. Shared ancestor counters can include other
workloads: an `oom_kill` increment or replacement PID does not identify a
particular Crystal Forge worker as the victim.

## Sustained stall and clearance

Recovery requires at least 180 seconds without a completed configuration and
three consecutive corroborating observations. Samples must be 30–60 seconds
apart. A completion, interval gap, unknown baseline, or incomplete sample
breaks the consecutive evidence run; repeated rapid reads cannot accelerate
the decision.

| Reason | Required corroboration |
| --- | --- |
| Reclaim pressure | Usage at least 90 percent of a finite high/max boundary, plus increasing high/max memory events or meaningful memory PSI |
| Blocked I/O | A persistent `D`-state process incarnation, aggregate CPU at most 1 percent of one core, and meaningful I/O PSI |
| Worker replacement | New direct-worker incarnations replacing missing workers, together with the reclaim-pressure evidence above |

Meaningful PSI requires both `avg10 >= 1` percent and interval stall time at
least 1 percent of elapsed sample time. A single high-usage observation,
low CPU, `D` state, replacement, or silent output interval is insufficient.
Confirmed configuration errors remain separate from resource evidence.

Clearance requires three healthy samples at the same spacing. Every finite
observed boundary must be below 80-percent usage, with no memory-event
increments and no meaningful memory or I/O PSI. CPU and worker-replacement
baselines must be known. Unknown samples cannot clear a latched stall.

## Idle assessment and one invocation deadline

The defaults remain `eval_output_idle_timeout_secs = 900` and
`eval_overall_timeout_secs = 3600`. One absolute overall deadline starts
before the first child. It covers normal phases, isolated recovery, small
fallback, build-preparation drain, and foreground cleanup. Replacing a child
does not start another 3600-second allowance. The deadline uses monotonic
time; it bounds foreground waiting, not guaranteed termination of every task.

An outer monitor remains active during all collection awaits, including
database work in output handlers, log flushing, and final child wait. At an
idle checkpoint, a complete sample no older than 60 seconds can waive
silence only when a direct worker has positive CPU progress, every sampled
member has a known CPU delta, and no member is in `D` state. Collector-only
or helper-only CPU activity does not satisfy this rule. Subsequent samples
must continue to justify the waiver. The waiver does not generate output,
advance the output timestamp, or extend the overall deadline.

Cancellation remains cooperative at two-second eligible loop boundaries.
A pending handler await can delay the actual cancellation query. This is
not a two-second response guarantee. The independent monitor does not wait
for the next cooperative poll to assess pressure or the overall deadline.

## Bounded phase transitions

1. Begin with the configured positive worker count and normal threshold.
2. On corroborated pressure, an unwaived idle checkpoint, or an unexplained
   phase/dropout failure, stop the child and confirm cleanup before replacement.
   Unexplained failure is not automatically classified as OOM.
3. Retain completed outcomes. Select only unresolved configurations from the
   same verified source inventory and evaluation scope.
4. Run configurations one at a time. Each configuration receives at most one
   isolated Crystal Forge retry during this invocation.
5. After one isolated completion and three healthy clearance samples, the
   controller may return to configured parallelism once. A second parallel
   stall makes serial recovery sticky for the remaining invocation.
6. Exhausted isolated work, no safe solo threshold, or the shared deadline
   terminalizes the remaining resource cohort. Do not restart the whole flake.

Valid policy Fail is a completed evaluation, not unfinished work. Neither
completed Pass nor completed Fail enters another recovery expression.
The optional Config-root observation remains independent; recovery must not
duplicate the root or claim that an unavailable optional snapshot succeeded.
The existing small silent-drop fallback retains its four-configuration,
concurrency-two, 180-second child limits, clipped by the invocation deadline.

Upstream [nix-eval-jobs v2.35.4](https://github.com/NixOS/nix-eval-jobs/blob/v2.35.4/src/nix-eval-jobs.cc)
has its own aggregate-budget scheduler and retry-alone mode. It samples RSS
every 200 milliseconds, independently of Crystal Forge's 30-second pressure
observations. The repository package remains v2.34.3 linked to Nix 2.34.8;
this documentation does not update that pin.

The v2.35.4 terminal solo outcome has free-form text in an ordinary error
response, not a dedicated resource error code:

```text
evaluation exceeded the memory budget of <digits> MiB (workers * max-memory-size) even when run alone
```

Crystal Forge recognizes that exact prefix, decimal budget, and suffix.
A recognized terminal solo error receives no additional isolated CF retry,
even if the upstream child exits zero. Generic signal, OOM, trace, or
configuration-error text is not equivalent evidence.

## Cleanup acknowledgement and quarantine

The process guard owns the inherited evaluator group and cleanup leases.
Collection does not terminate or disarm the guard. After collection stops,
cleanup runs outside its monitor race but within the remaining invocation
deadline. A replacement requires direct-child reap, confirmed group absence,
and acknowledgement of preparation-child cleanup.

A signal request, EOF, leader exit alone, diagnostic scan, or task abort is
not group-cleanup acknowledgement. A `D`-state member can remain after
SIGKILL. If cleanup is unknown or still pending at the deadline, the guard
quarantines the group and retains the existing heavy-Nix lock ownership.
Quarantined cleanup can outlive the invocation deadline and continue to hold
those locks. The deadline does not force a pending `D`-state task to exit.
The runtime reaper releases that ownership only after confirming reap and
group absence. It does not start a competing replacement or a verification
supervisor. A cancelled reaper must not manufacture an acknowledgement.

Preparation task abort acknowledgements have a separate bounded allowance;
that allowance is not extra process-reap time and does not reset the overall
deadline. A preparation child can retain its cleanup lease after executor
acknowledgement. This barrier covers processes that keep the inherited
group; it is not containment for a process that escapes the group.

## Exact-attempt partial persistence

Bounded resource exhaustion sets both the commit and its active attempt to
`failed`, never `complete`. The finalizer rechecks current-attempt identity,
cancellation, and supersession inside its transaction. It validates exact
configuration, target, `.drvPath`, expected output, agent metadata, and policy
evidence before retaining completed results.

The durable marker is the JSON value `policy_results.evaluation_attempt`,
compared to the expected/current attempt as a JSON integer. It is not a
name-only match or a serialized-text approximation. Completed Pass/Fail,
available/unavailable snapshot observations, and existing build jobs remain
valid only for that verified current-attempt cohort. Older or scope-excluded
rows cannot authorize retention. Invalid retained evidence rolls back the
terminal transaction.

Unfinished selected configurations receive policy Error with
`resource_pressure` evidence, not a fabricated configuration Fail. Confirmed
syntax/evaluation failures retain their separate deterministic diagnostic and
terminal outcome. Internal `ResourceFailure` is persisted as the existing
`transient` database class for compatibility, with a `resource_pressure: `
diagnostic prefix. It is not a new builder wire value.

Exhausted resource recovery never queues an automatic whole-flake attempt,
even though the normal global default allows one evaluation retry. This
preserves same-attempt completion evidence and the approved finite recovery
budget. Change resource demand before a deliberate manual retry.

Exact completed, policy-eligible derivations whose build preparation remains
pending or failed can still enter the existing preparation reconciler after
resource-terminal failure. Admission rechecks the marker, failed attempt,
scope, agent enablement, policy eligibility, current commit, and GC-root
requirements. Cancellation, archival, supersession, missing markers, rejected
policy, or unverified targets block this exception. It does not create work
for unfinished configurations or generally admit failed commits.

The runtime stages checked identity, policy evidence, and captures before its
persistence await. If database COMMIT becomes durable but caller acknowledgement
is interrupted, `reconcile_unacknowledged_completion` checks durable evidence
before runtime selects another expression. The read acquires the existing
snapshot-writer, queue, commit, and active-attempt locks. It creates no
derivations, jobs, or snapshots. Missing or older markers return false;
contradictory current evidence, cancellation, and supersession are errors.

If runtime cannot complete that read within its original deadline, the server
passes the unacknowledged candidates to the partial-failure finalizer. Under
the same current-attempt locks, the finalizer uses the shared
`completion_committed_tx` proof to merge only committed candidates and their
captures before computing the unfinished resource cohort. Never-committed
candidates remain unfinished. A candidate alone cannot suppress evaluation,
authorize capture publication, or become a completed result.

Persisted and broadcast completed/remaining counts come from this transactional
proof, not pre-reconciliation checkpoint guesses. Resource observations for a
proven completed candidate are removed from the unfinished summary. The
terminal handoff preserves pending preparation without queuing a job before
the normal GC-root and policy-eligibility checks.

## Operator progress and deployment diagnosis

Review `evaluator_resource_plan`, pressure-sample completeness and baselines,
the bounded recovery-phase worker/memory/count fields, remaining-resource
summaries, and quarantine/cleanup acknowledgement messages together. A
quarantined cleanup can keep heavy-Nix work serialized after the attempt's
failure response. Do not assume that a stable API or a failed attempt means
the old group is absent.

For deployment diagnosis, confirm the actual loaded configuration and binary
versions, then correlate a stable PID plus start time, parent/group identity,
CPU deltas, state, charged memory, event deltas, and PSI across samples.
Collect only nonsecret numeric evidence; do not collect argv, environment,
expressions, or credential-bearing raw logs. Distinguish cgroup throttling,
kernel OOM evidence, upstream budget intervention, and CF isolation. Shared
ancestor OOM counters and changed worker PIDs alone cannot prove who killed
which worker. Unknown diagnostics require investigation, not a health claim.
