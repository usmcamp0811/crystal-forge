# Upgrade: exact-release version transition

`version-transition` implements the owner-approved TASK-490.23 D2 procedure.
The predecessor is tag `v0.3.1`, commit
`f9dd6b9c79530d3f911203645207bf6251552aba`. The release's Cargo manifest says
`0.3.0`. The scenario records both identities and checks running executables
against the genuine package paths. A version string alone is insufficient.

## Preparation and execution

The root `upgrade-predecessor` input uses an exact HTTPS GitLab revision and its
own release lock graph. GitLab CI needs read access to that repository. Do not
make the predecessor follow current inputs. Do not update current locked inputs
when preparing the predecessor.

Package and closure preparation runs online before VM execution. It fetches
the locked input sources, Cargo dependencies, production packages, target
baseline and full derivation recipes. The VM scenario blocks public egress,
disables external substituters and remote builders, and disables fallback.
The local Git service and signed SSH Nix store are invocation-owned fixtures.
Runtime-authored nonces distinguish actual builds from immutable preparation.
The release builder uses `build.offline = false` because its implementation
passes the unsupported `--offline` flag to legacy `nix-store`. The VM egress
rules and empty substituter/builder settings still enforce offline execution.
Current builders use their supported `build.offline = true` configuration.
The release worker receives a derivation path, not a derivation archive. Its
fixture prerequisite is the real evaluator-created recipe and source closure in
the worker's store. Before releasing an old claim, the scenario uses ordinary
`nix copy --derivation` through the local authenticated store to stage those
inputs. Output-absence assertions surround staging. The old worker must realize
and publish the runtime-authored candidate itself. Current builders receive
their inputs through current server APIs.

Run the normal required gate:

```sh
sh packages/ci/integration-owner-build.sh integration-upgrade
```

The helper uses local `--max-jobs 1 --cores 2` builds, preserves the aggregate's
actual exit status, and copies available evidence to
`integration-evidence/integration-upgrade/version-transition/`. A focused gate is
`checks.x86_64-linux.integration-upgrade.scenarios.version-transition`.
Its `.run` output is evidence only. It is not a passing gate.

## Controlled maintenance boundary

All five existing roles belong to one fresh invocation. PostgreSQL remains in
the same controller and cluster throughout. The target retains its writable
installation, registered SystemID and persistent signing key.

The predecessor's unmodified NixOS module creates its normal configuration.
Real API activity creates credentials, registration, signed installed-state
history, audit records, queued work and an interrupted claim. The genuine release
builder realizes a runtime-authored output and signs and publishes that output.
The scenario verifies the nonce, trusted Nix signature and completed publication
records, then preserves their job/output/destination relationships across migration.
The installed-state observation describes the existing target, not activation of
that producer output. The release stores its authenticated initial report as a
full state row. Separate heartbeat rows can be absent before the ten-minute poll;
any existing heartbeat/state relationships must survive migration. A genuine
old-agent/current-server heartbeat is mandatory after cutover.
In the pinned release, `packages/default/src/bin/agent.rs` signs a full
`SystemState` and POSTs it to `/agent/heartbeat` with `startup` context. Its
periodic loop sends the same full payload format with `heartbeat` context after
600 seconds. The release handler in `handlers/agent/heartbeat.rs` authenticates
that full payload before `models/agent_heartbeats.rs` selects a full state row or
a heartbeat row. The post-cutover restart probe also sends a full state report.
An `agent_heartbeats` row proves persisted liveness and state linkage; it does
not prove that a thin heartbeat protocol message was sent. This scenario does
not claim old-agent thin-message compatibility.
The release API builder also needs a PostgreSQL pool. Only the predecessor phase
permits that access.

Maintenance stops predecessor writers and builders. Builder specialisations
install current packages and normal API-only configuration while the units stay
stopped. The scenario removes old generated database configuration and blocks
builder database transport before current-server startup. The current server
runs normal SQLx migrations. Readiness precedes current-builder startup. Neither
current builder receives a database-credential exception.

The compatibility deployment source retains the exact predecessor agent package.
The scenario checks the running executable before and after actual activation
and signed reporting. Only the deliberate operator specialisation replaces that
agent. A subsequent source revision explicitly uses the current package. No timed
compatibility expiry or version-refusal policy is introduced.
The release agent hard-codes a 600-second periodic heartbeat. A normal restart
produces the explicit post-cutover heartbeat observation. Old-agent deployment
waits cover its next ordinary poll and activation. These test bounds do not
define an agent compatibility expiry. Current recovery uses the existing
exact-commit re-evaluation and terminal-job retry APIs.

Operator-managed cache credentials use `/var/lib/upgrade-credentials`, outside
the application's recursive tmpfiles rule. The scenario checks their private
file modes after activation and uses the same credentials for publication.

Release history consists of observed state rows and administrator audit records;
the release predates `system_events`. Preserve those original identities and
relationships. New current deployments must also produce their correlated
terminal success event. Queued and interrupted logical work must recover with
valid successor relationships and one authoritative success. Physical execution
can repeat. A fresh-timestamp, correctly signed release-protocol completion
probe for the actual interrupted claim must have no effect after replacement.
Failed or incomplete publication must not authorize deployment.
Legacy signed state observations remain observable through the existing
`agent-events` read. They must not be reclassified as successful canonical
deployment events merely to populate the current deployment timeline. The
scenario verifies their SQL identities and their observation API provenance;
the current history UI must show actual current-server deployment successes.

The publication-negative case is bound to a real successful current-server
deployment. Its new candidate changes only the operation nonce (and, when
applicable, the already-proved agent package selector). The case verifies native
evaluation, policy admission, actual realization and the worker's publication
failure before interpreting the API result. D2 does not specify an HTTP 4xx
classification. The case records an observed HTTP 500 but never uses that status
as sufficient proof: completion/publication state must remain unauthorized;
protected system/deployment state, candidate events and actual target state must
remain unchanged, including after a genuine signed agent delivery poll.

Credential diagnostics contain service/process UID and GID, configured path
sources and file ownership/modes only. They do not read private key contents.
The released agent runs as root and accepts its configured `client.private_key`
path; its declared state and runtime directories are separate from the
server/builder state tree. TASK-502 concerns the recursive permission rule for
the latter tree. It does not explain the historical old-server activation failure.
The released commit-deployment API writes a Git SHA into `desired_target`, its
heartbeat handler returns that string unchanged, and the released agent rejects
the SHA before cache copy because it accepts store paths. The same exact agent
accepts the current server's resolved store path. D2 requires real predecessor
completed records/history and old-agent deployment with the current server. The
mandatory producer case therefore uses completed native build/publication and
authenticated installed-state history. It does not run the extra old-server
manual commit deployment experiment. No case is skipped or marked as an expected
failure; all thirteen remain mandatory. The required old-agent/current-server
activation and signed report remain separate hard assertions before agent rollout.

The historical experiment at `bc6aa6dc` executed thirteen cases with twelve passes
and one failure. Its raw evidence remains at
`/nix/store/nif62m1y0swiscy61z9l2wcc4kfak1h4-vm-test-run-cf-it-upgrade-version-transition-run`
and in `/tmp/opencode/task49023-bc6aa6dc-evidence.tar.gz` on the task host.
Those artifacts document the predecessor limitation; they are not a passing
baseline or a predecessor source repair.

The first corrected-baseline run also remains available in
`/tmp/opencode/task49023-d2-baseline-evidence.tar.gz` on the task host. All thirteen
cases executed: ten passed and three failed. A test-only requirement for a
nonempty predecessor heartbeat table prevented the completed-history receipt and
caused dependent migration/browser assertions to fail. The release source and
the signed startup report establish why full state history is the correct
producer evidence. The mandatory post-cutover liveness and deployment proofs
are unchanged.

After real registration and an initial signed state, independent cases retain
build/interruption failures and continue through maintenance when the database
is usable. Missing completed-history receipts remain failed prerequisites. The
scenario does not create replacement successful records. The final assertion
fails the owner if any mandatory case failed. Startup or migration failures stop
dependent work. Compatibility, migration or
recovery defects require separate production-fix authority; do not weaken their
assertions or patch predecessor code.
