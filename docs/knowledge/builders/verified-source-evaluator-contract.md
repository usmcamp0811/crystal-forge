---
type: Design Specification
title: "Verified-source evaluator contract (source_re_evaluate_verified)"
description: "Specifies the verified-source flow where the builder re-evaluates a canonical source archive and compares its .drvPath to the server value, including the evaluator fingerprint, next-job 409 reasons, and rolling-upgrade behavior."
tags:
  - crystal-forge
  - builder
  - evaluator
  - verified-source
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/multi-builder-api.md at commit 3b23d36f"
    title: "Multi-Builder API Documentation"
---

# Verified-source evaluator contract (source_re_evaluate_verified)

## `source_re_evaluate_verified`

`source_re_evaluate_verified` is the verified source strategy. It keeps the server authoritative while avoiding monolithic derivation-closure transfer as the common path.

Flow:

1. The server fetches the exact commit into its credentialed bare mirror. It
   exports only the tracked Git tree and ingests that tree with the canonical
   `crystal-forge-source-v1-<commit>` name. It evaluates the resulting immutable
   store flake in pure mode with lock mutation disabled and IFD set explicitly:

   ```bash
   nix-eval-jobs --expr '<authoritative expression>' \
       --option pure-eval true \
       --option allow-import-from-derivation true \
       --meta --apply 'derivation: derivation.meta.policies' \
       --workers <n> --max-memory-size <MiB>
   ```

   The resulting `.drvPath` is the server-authorized build-plan fingerprint. The server does not need `nix build --dry-run` for this identity. The authoritative evaluator does not receive `BuildConfig` realization options such as sandbox, offline mode, substitution policy, max jobs, cores, max-silent-time, or build timeout. Those settings control realization and must not alter evaluation semantics.

2. The server sends the full commit, lock digest, canonical store name, source
   NAR hash, artifact format, artifact digest and size, evaluator contract,
   flake target, and expected
   `.drvPath`. The NAR hash and canonical name are the portable source identity.
   The server's physical store path is diagnostic only.

3. The builder obtains the canonical artifact through the job-owned API endpoint.
   The manifest repository URL has embedded user information, passwords, query
   parameters, and fragments removed. The builder does not use the URL to fetch.

4. Before polling, the builder probes and caches its actual Nix version and
   `builtins.currentSystem`. Each signed `NextJobRequest` advertises those values
   with the contract version, pure-evaluation setting, lock-mutation setting,
   IFD setting, and source materialization schema. The server compares the full
   capability with its authoritative `nix-eval-jobs` fingerprint before queue
   lookup or claim. Legacy requests and any mismatch receive HTTP 409 with the
   `incompatible_evaluator` reason and do not mutate a queued job.

5. The builder verifies artifact size, SHA-256, format, lock digest, store name,
   and NAR hash. It rejects incompatible Nix version, purity,
   lock-mutation, IFD, or materialization settings before evaluation. It then
   evaluates the same NAR-qualified store reference as the server before building:

   ```bash
   drv=$(nix eval --raw --no-write-lock-file \
      --option pure-eval true \
      --option allow-import-from-derivation true \
     'path:/nix/store/<source>?narHash=<percent-encoded-SRI>#nixosConfigurations.<host>.config.system.build.toplevel.drvPath')
   ```

6. The builder compares `$drv` to the server-provided expected `.drvPath`. A mismatch fails before any build starts with `derivation_mismatch`.

7. If the strings match, the builder builds the exact verified derivation object:

   ```bash
   nix build "$drv^*"
   ```

   The important property is eval → compare → build, not build → inspect.

This strategy verifies derivation identity/build-plan equality. It does not prove bit-for-bit output reproducibility; output reproducibility is a separate concern.

The evaluator fingerprint covers contract version, linked Nix version,
`builtins.currentSystem`, pure evaluation, lock-file mutation policy, IFD policy,
and source materialization schema. Server-only worker count, evaluator memory
limit, outer process timeout, and cache-status reporting are resource or
diagnostic controls. They can stop an evaluation or add metadata, but they
cannot change a successful `.drvPath`.

A post-claim fingerprint mismatch remains a defense-in-depth check. The server
releases that job to the queue without consuming retry budget or failing the
shared derivation. A pre-upgrade queued job with no usable contract-v1
publication is instead failed with the server-owned `server_failure_code` value
`evaluator_contract_obsolete`; selection continues to the next queue candidate.
Builder logs and failure requests cannot set this field. A later authoritative
re-evaluation can revive the unique row only after source publication and pure
evaluation succeed and the derivation reaches `DryRunComplete`. The same
transaction that queues the row consumes the code by setting it to null. Other
terminal failures retain normal retry and manual-requeue semantics.

Recommended controls:

- Keep source identity immutable: full commit hash, lock digest, artifact format,
  artifact SHA-256 and size, canonical store name, and source NAR hash.
- Retain canonical server artifacts independently of job completion. Dispatch
  validates a published artifact before atomically claiming its exact job.
- Extract builder artifacts only through the bounded contract-v1 tar validator
  and remove each unique temporary directory after evaluation.
- Prefer server-bundled inputs for locked-down or GovCloud-style builders with no internet egress.
- Do not place broad private Git credentials on every builder.
- Record or pin the Nix version/evaluator fingerprint across server and builders.
- Never use `--impure` for this strategy. Impure evaluation can observe host
  `nix.conf`, environment variables, and files and can authorize a host-specific
  derivation.
- P2 hardening: launch authoritative and builder evaluators with an explicit
  environment allowlist so ambient credential variables cannot influence input
  resolution. Contract version 1 does not yet enforce this process boundary.

Expected pre-build failure phases include `source_fetch`,
`source_identity_mismatch`, `evaluator_incompatible`,
`source_input_availability`, `evaluation`, `derivation_mismatch`, and
`path_materialization`.

During rolling upgrades, new builders advertise evaluator contract version 1.
Old builders and old request payloads default to version 0. The server returns
409 before job claim when the configured verified-source strategy requires a
contract the builder cannot validate. It does not silently select another
strategy. A new builder can still poll an old server because old serde readers
ignore the additive capability field.

Every next-job 409 response has a JSON body with a stable `reason` field:

- `unsupported_execution_strategy` means the builder did not advertise the
  server's configured strategy.
- `incompatible_evaluator` means the complete builder and server evaluator
  fingerprints differ.
- `incompatible_source_delivery` means the server delivery mode cannot satisfy
  the selected evaluator contract.
- `source_materialization_cancelled` means canonical source preparation was
  cancelled before claim.

The first three checks occur before queue lookup. Source cancellation occurs
before the atomic claim. None of these responses claims or mutates a queued job.

## Related concepts

* [Remote builder execution strategies](remote-build-execution-strategies.md) - Explains the remote build execution strategies (source_re_evaluate_verified, server_derivation), the recommended default, source delivery modes, delta derivation materialization, and the forwarded-HTTPS rule for credential-bearing cache push.
* [Builder failure phases and retry strategy](builder-failure-phases-and-retry.md) - Lists the pre-build failure phases a builder reports (source_fetch through build), which of them retry or fail permanently, and the priority-weighting retry and max-retries rules for build jobs.
* [Builder threat model](../security/builder-threat-model.md) - Analyzes what an attacker obtains from a compromised builder, a malicious job claim, source archive tampering, and request replay, and which defenses (digest check, derivation_mismatch, timestamp window) apply.
* [Builder API: job lifecycle endpoints](../api/builder-job-lifecycle-api.md) - Documents the builder-signed endpoints for heartbeat, next-job polling (including 409 evaluator conflicts), derivation manifest and delta/full derivation archives, job completion, failure, and log append.
