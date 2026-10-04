# Niks3 cache integration

Run from the task worktree:

```sh
nix eval --raw .#checks.x86_64-linux.niks3-cache.drvPath
nix build --no-link -L .#checks.x86_64-linux.niks3-cache
```

While new files are untracked, use `path:.#` instead of `.#`. Git-backed flake
evaluation excludes untracked files, including new Rust modules and migrations.
Do not update the lock file to make this check pass.

The pinned package is **Niks3 1.6.0**. Crystal Forge overrides its `nix`
dependency with `pkgs.nix-eval-jobs.nix`. This retains Niks3's upstream version
and preserves the exact evaluator Nix when the CLI starts a child process.

## CI

The `.gitlab-ci.yml` `flake-check` matrix includes `CHECK_NAME: niks3-cache`.
The generated job `flake-check: [niks3-cache]` is blocking on merge requests and
`main`, uses the existing `nix` runner tag, and builds
`.#checks.x86_64-linux.niks3-cache`. The assigned runner must provide KVM to the
Nix builder. Report the exact job's runner or KVM failure; a fixture run or
another check does not replace this gate. Matrix membership and earlier local
runs do not establish a pass for the exact commit under review.

## Full integration gate

The check uses four isolated NixOS VMs: cache, server, remote API builder, and
agent. PostgreSQL and Garage data exist only in the VM disks. The host's
development database is not used. Closure-only Nix store images prevent the
agent from seeing outputs through a host-store mount. KVM must be available to
the Nix builder.

Queue records and evaluated identities are seeded; flake evaluation is not
part of this check. Each variant dispatches a real derivation to the packaged
remote builder. The check requires a successful signed completion from that
registered builder and
a completed publication for the dispatch-bound destination ID. The server must
verify readability and signatures in its temporary store before recording
publication. The real agent receives its read configuration over signed
heartbeats through a verified HTTPS proxy and
pulls an output that is absent from its local store.

Every variant has an assigned Niks3 destination and an enabled public global
`Http` destination named `a-global-public`. The global destination sorts first.
Builder dispatch and completed publication must retain the assigned ID and
database provenance. A signed capable heartbeat must return only the assigned
read cache. Before starting the packaged agent, the fixture sends a genuinely
legacy flat JSON body with no `capabilities` field, signed with the registered
Ed25519 key. Spoofed capability headers do not change that body. The response
must have a null desired target and empty runtime caches, with no global
fallback, including for public Niks3. The pending request must remain unclaimed
and the stored desired target must remain intact. The actual packaged agent
then advertises support in its signed body, claims that request, and pulls.

| Variant | Write authentication | Read endpoint |
| --- | --- | --- |
| `token-public` | Static token file | Public native Niks3 read proxy |
| `mtls-public` | Write client certificate | Public native Niks3 read proxy |
| `token-private` | Static token file | Read-only mTLS proxy |
| `mtls-private` | Write client certificate | Read-only mTLS proxy |
| `token-public-proxy` | Static token file | Public read-only TLS proxy |

The fixture uses separate write, read, and unauthorized client subjects. Both
signing keys must appear in published narinfo. Private reads reject anonymous
requests and the unauthorized subject. Read certificates cannot access the
write API through the read-only proxy. Invalid write tokens and unauthorized
write certificates must fail.

The check also requires server-local CVE materialization of a missing output
from the completed cache publication. The remote builder's scanner capability
is disabled to prevent it from racing that local scan. A scan may fail after
materialization because the VM has a synthetic target and no external
vulnerability database. Such a failure does not establish successful CVE
analysis. The assertion proves output restoration and terminal process cleanup.
The rebased lifecycle can create an active post-build scan intent even when
automatic scanning is disabled. The manual fixture request reuses that active
identity under the active-scan unique index. It changes the request trigger to
`manual` without fabricating scanner results or successful scan evidence.

An unrelated environment has an enabled private cache with an unauthorized read
identity. The agent must fail to pull the private target when moved into that
environment, then succeed after its original environment is restored. Credentials
are encrypted in seeded destination rows. Service logs, persisted scan metadata,
diagnostic events, and temporary credential-directory cleanup are checked
without printing secrets.

The fixture provides a valid deterministic agent/builder signing keypair and an
explicit VM-local database host. Deployment requests include a fresh timestamp
and a pending request row. The agent's startup deployment delay is zero in this
check so each restart can exercise a pull immediately. A complete deployment
section is supplied through service environment variables. This avoids the
production module's current string-duration serialization mismatch; the Rust
deployment config expects integer seconds. Standalone agent tmpfiles also need
the fixture-supplied `crystal-forge` user.

## Fixture diagnosis

This smaller command can run while shared Rust integration is incomplete:

```sh
nix build --no-link -L .#checks.x86_64-linux.niks3-cache.fixture
```

The fixture check runs PostgreSQL, Garage, Niks3, TLS read proxies, and a CLI
probe in disposable VMs. It tests real token/mTLS publication, public/private
reads, invalid write tokens, read-only certificate rejection for writes,
wrong-subject read rejection, each signing key independently with a separate
local Nix store, rejection of an untrusted signing key, and service-log audits.
**A fixture pass does not satisfy the full remote-builder/agent gate.**

All fixture tokens and private keys are deterministic, public, nonproduction
test data. TLS certificates are issued at derivation build time. Niks3 debug
logging is disabled. These fixtures must never be used on deployed systems.

## Limits

- The target has a no-op activation script. Agent pull is tested; a complete
  NixOS generation switch and reboot are not tested.
- CVE materialization covers the server-local worker. Remote scanner credential
  delivery and a successful vulnerability analysis are not covered here.
- Environment isolation is exercised through cache selection and a negative
  agent pull. The check does not inspect a captured heartbeat body for every
  possible secret field.
- Invalid certificates use a CA-signed unauthorized subject. Expired and
  untrusted-CA client certificates are not separately covered.
- External authentication providers and Bearer/OIDC reads are outside TASK-470.
- Niks3 1.6.0 logs a prefix/suffix preview of rejected tokens at WARN level.
  The invalid token is deliberate nonproduction test data. The audit rejects
  configured tokens, private-key material, and presigned upload URLs; it does
  not establish that upstream rejected-token previews are absent.
- Garage INFO access logs contain presigned URLs. The fixture uses WARN logging
  to keep those upload capabilities out of the final service-log audit.

Run the packaging gate separately as documented in
[`builder-evaluator-packaging`](../builder-evaluator-packaging/README.md).
