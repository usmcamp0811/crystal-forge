---
type: Operator Guide
title: "Crystal Forge — S3 Cache (MinIO) Quickstart"
description: "Shows how to push Nix store paths to a MinIO-backed S3 cache and use it as a substituter, including path-style addressing, AWS environment variables, the NixOS cache configuration snippet, and troubleshooting."
tags:
  - crystal-forge
  - cache
  - s3
  - minio
  - nix-copy
implementation_status: implemented
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/s3-cache-notes.md at commit 3b23d36f"
    title: "Crystal Forge — S3 Cache (MinIO) Quickstart"
---

# Crystal Forge — S3 Cache (MinIO) Quickstart

Use this to push Nix store paths to a MinIO-backed S3 cache and consume them later.

## Requirements

- **MinIO** running and reachable (e.g. `http://s3Cache:9000`)
- **Bucket** created (e.g. `crystal-forge-cache`)
- **Anonymous read** enabled on the bucket (needed for clients to fetch `narinfo` / `nar`):

  ```bash
  mc alias set local http://s3Cache:9000 <ACCESS_KEY> <SECRET_KEY>
  mc mb local/crystal-forge-cache || true
  mc anonymous set download local/crystal-forge-cache
  ```

- **AWS credentials** for pushes:

  - `AWS_ACCESS_KEY_ID`
  - `AWS_SECRET_ACCESS_KEY`

## Why this setup

- MinIO often needs **path-style** S3 addressing; otherwise the SDK tries `bucket.endpoint` which breaks with raw IPs / test nets.
- Explicit **endpoint** and **region** make signing deterministic.

## Environment (recommended)

```bash
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_REGION=us-east-1
export AWS_EC2_METADATA_DISABLED=1
```

## Push: two working forms

### A) All-in-URL (portable)

```bash
nix copy --extra-experimental-features nix-command \
  --to 's3://crystal-forge-cache?endpoint=http://s3Cache:9000&scheme=http&region=us-east-1&force-path-style=true' \
  /nix/store/<path-or-closure>
```

### B) Via env + short URL

```bash
export AWS_ENDPOINT_URL=http://s3Cache:9000
export AWS_S3_FORCE_PATH_STYLE=1

nix copy --extra-experimental-features nix-command \
  --to s3://crystal-forge-cache \
  /nix/store/<path-or-closure>
```

> Tip: Prefer the **hostname** (`s3Cache`) instead of a hardcoded IP so tests don’t depend on DHCP assignments.

## Use as a substituter (reads)

```bash
nix build . \
  --substituters 's3://crystal-forge-cache?endpoint=http://s3Cache:9000&scheme=http&region=us-east-1&force-path-style=true'
```

## Crystal Forge (NixOS) snippet

```nix
services.crystal-forge = {
  build.systemd_properties = [
    "Environment=AWS_ACCESS_KEY_ID=minioadmin"
    "Environment=AWS_SECRET_ACCESS_KEY=minioadmin"
    "Environment=AWS_REGION=us-east-1"
    "Environment=AWS_EC2_METADATA_DISABLED=true"
  ];

  cache = {
    cache_type = "S3";
    push_to =
      "s3://crystal-forge-cache?endpoint=http://s3Cache:9000&scheme=http&region=us-east-1&force-path-style=true";
    push_after_build = true;

    # Optional tuning
    s3_region = "us-east-1";
    parallel_uploads = 2;
    max_retries = 2;
    retry_delay_seconds = 1;
  };
};
```

## Troubleshooting

- **`curlCode: 6, Could not resolve hostname`**
  You’re hitting virtual-hosted style (`bucket.endpoint`). Add `force-path-style=true` or set `AWS_S3_FORCE_PATH_STYLE=1`. Ensure `endpoint=` is set.

- **Auth errors**
  Confirm env vars are present in the **same scope** as `nix copy` (Systemd unit vs shell). Disable IMDS: `AWS_EC2_METADATA_DISABLED=1`.

- **Anonymous fetch fails**
  Re-run: `mc anonymous set download local/<bucket>`.

- **MinIO not ready**
  Wait for `/minio/health/live` to go 200 OK, or add a small retry loop before pushing.

## Related concepts

* [Remote builder execution strategies](../builders/remote-build-execution-strategies.md) - Explains the remote build execution strategies (source_re_evaluate_verified, server_derivation), the recommended default, source delivery modes, delta derivation materialization, and the forwarded-HTTPS rule for credential-bearing cache push.
* [Builder filesystem layout, firewall rules, and network-constrained configuration](../operations/builder-network-and-filesystem-requirements.md) - Gives the builder host filesystem layout and cleanup guarantees, the firewall rules required per execution strategy, the server inbound rules, and example configurations for maximum isolation and for colocated deployments.
