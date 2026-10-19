# Delivery integration owner

`delivery-lifecycle` explicitly registers one evolving five-role scenario:
controller with PostgreSQL, local Git/SSH-store services, two API-only builders,
and a real NixOS target agent. These boundaries represent distinct stores and
credentials, not implementation tasks. No scenario consumes Access runtime data.

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
the runtime nonce-specific output is never built on the host. Selected direct
build inputs are retained through immutable Nix
metadata, not deep recipe contexts that force every transitive output. The
authored source and baseline use the same production evaluator/Nix pair. Public/NAT egress
is blocked; local role traffic remains available. Native builder completion
requires publication, so the scenario supplies a real authenticated SSH Nix
store. This does not claim complete cache-adapter or deployment coverage.

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
TASK-496 separately records heartbeat
classification falling back to full-state persistence after a database error;
periodic reporting does not establish correctness of that optimization.

Later scheduling, source verification, publication variants, activation,
rollback and recovery extend compatible sections of this lifecycle. Accepted
remaining obligations and exact execution evidence belong in the existing
[coverage map](../../docs/knowledge/testing/integration-coverage-map.md).
