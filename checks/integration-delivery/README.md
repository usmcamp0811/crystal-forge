# Delivery integration owner

`delivery-lifecycle` explicitly registers one evolving five-role scenario:
controller with PostgreSQL, local Git/SSH-store services, two API-only builders,
and a real NixOS target agent. These boundaries represent distinct stores and
credentials, not implementation tasks. No scenario consumes Access runtime data.

**Current gate: failed at TASK-499.** The connected manual deployment increment
executes thirteen passing cases and a mandatory retained-rollback case that
fails because native deployments have no certified retained artifact binding.
No rollback, skip, expected failure or completed-overhaul claim is implied.

The driver establishes its own administrator and environment, authors a real
nonce-bearing commit, registers runtime-generated public keys, and starts the
target agent. `fragments/source-build.py` then requires native evaluation of
that exact SHA and attempt, independently queries the authoritative derivation's
output, and checks both filesystem absence and Nix invalidity on every node.
Only afterward may builderA claim and build the job. BuilderB starts after
completion so it cannot race for the proving job. Generated identities pass
through write-once receipts; private keys remain inside the VMs.

The source contains a real NixOS configuration and an unchanged copy of the
production module. Locked nixpkgs and baseline dependencies are prepared, but
the runtime nonce-specific output is never built on the host. Raw archive
transport requires a valid complete recipe closure, including bootstrap source
objects. One native baseline preparation supplies that closure to compatible
roles; its cold preparation cost remains a minimization constraint. The source
generator itself does not expand recipe contexts. The authored source and
baseline use the same production evaluator/Nix pair. Public/NAT egress
is blocked; local role traffic remains available. Native builder completion
requires publication, so the scenario supplies a real authenticated SSH Nix
store. The selected manual activation path follows below; complete cache-adapter
and deployment-mode coverage remain open.

`fragments/network-boundary.py` checks a controlled outside network on `eth2`
after the source/build cases. The same five VMs have a second test VLAN; the
services VM hosts an IPv4/IPv6 HTTP probe. The `net-control` UID can reach only
that probe address and TCP port 9080. Other identities remain blocked. Each
controller, builder and target probe requires the `eth2` route, a successful
same-route control request, a failed product-actor request, and an increased
counter on the exact rejection rule. IPv6 neighbor discovery remains available.
Local Git, API and authenticated SSH connections must still work. This proves
the controlled boundary, not host-daemon network isolation or every external
destination. No deployed cache is contacted.

`fragments/publication.py` consumes the exact CF-built output after normal
publication. A separate rooted store inside services receives every copy
through an untrusted Unix-daemon client. The receiver trusts one runtime-only
signing identity. The driver signs and copies dependencies, but leaves the
input-addressed target cold and signed only by an untrusted publisher. The
target must fail specifically for lack of a trusted signature and remain
physically absent and invalid. Signing that same output must make the same copy
succeed without changing receiver policy. NAR hash, size, references and nonce
must remain unchanged. Root signs source metadata; no privileged client imports
into the receiver. This proves the signature boundary separately from the
existing trusted SSH publication account.

`fragments/activation.py` and `activation.cjs` extend that same lifecycle through
two distinguishable CF-managed revisions. The production UI requests each
deployment. The target must independently lack its runtime output before agent
acquisition, then run the exact output and activation-managed `/etc` marker.
The profile, unchanged boot identity, real agent key, succeeded CF request,
exact history event and two later heartbeats must agree. Each transition adds
one full state; unchanged reports must not add more. Browser reload verifies
the actual generation and durable success history, not merely a queued or
Started response. The existing Admin supplies authorized operator capability.

The booted target and Git candidates share pinned QEMU/test instrumentation,
network isolation and the production agent. Switching is enabled; GRUB is
disabled for this direct-boot live-switch test. Reboot support is not proved.
Only the fixed immutable baseline runtime closure is prepared on the target;
runtime-authored candidates remain cold. This does not prove cold acquisition
of every dependency from a warm cache (TASK-498). The normal startup delay is
retained; nondefault deployment-block generation exposes TASK-497.

`fragments/recovery.py` requires fresh server-owned rollback eligibility after
both deployments. The production History action and `recovery.cjs` must select
the retained first artifact; no direct state writes or fabricated retention can
supply it. Current native evaluation produces no available V1 authority, while
Config inspection produces independently selected V2 evidence. Deployment
binding and retention still require V1. TASK-499 blocks that prerequisite, so
the rollback browser action and restoration assertions have not executed.
TASK-500 separately tracks Started/success history presentation and the design
strategy-label discrepancy; screenshots do not approve those differences.

```bash
sh packages/ci/integration-owner-build.sh integration-delivery
```

The normal aggregate requires every case. Its `.run` output is evidence only.
During development, the real builder exposed the unsupported
`nix-store --offline` flag (TASK-495). The owner authorized a separate product
fix, developed in its own worktree and committed independently. The normal
Delivery aggregate then passed all eight cases locally with `offline = true`,
real signed publication and all success/output assertions retained. The coverage
map records exact source and evidence; this is not complete Delivery coverage.
The earlier local continuation based on `d11de059` passed ten cases, including
signature rejection/acceptance and the controlled network proof. That source
includes TASK-496's separate repair. The target assertion requires two real
heartbeat rows referencing the exact initial state, no duplicate unchanged full
state, and no comparison database error. Full-state fallback cannot satisfy the
assertion. A genuine changed-state transition and later heartbeat correlation
were then open. The committed ten-case increment `3db47c89` subsequently passed
hosted pipeline 2965867360. The connected fourteen-case increment has separate
dirty-tree evidence and remains failed at rollback. The coverage map records
exact commands, source identities, artifacts and limits.

Later scheduling, source verification, publication variants, activation,
rollback and recovery extend compatible sections of this lifecycle. Accepted
remaining obligations and exact execution evidence belong in the existing
[coverage map](../../docs/knowledge/testing/integration-coverage-map.md).
