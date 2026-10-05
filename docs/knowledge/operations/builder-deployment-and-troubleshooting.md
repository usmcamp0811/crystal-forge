---
type: Operator Guide
title: "Builder deployment, configuration, and troubleshooting"
description: "Explains how to register and deploy a builder (prerequisites, keypair generation, builder configuration, polling loop pseudocode) and how to troubleshoot missing jobs, authentication failures, and jobs that do not retry."
tags:
  - crystal-forge
  - builder
  - deployment
  - troubleshooting
implementation_status: implemented
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:57:42-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/multi-builder-api.md at commit 3b23d36f"
    title: "Multi-Builder API Documentation"
---

# Builder deployment, configuration, and troubleshooting

## Builder Deployment

### Prerequisites

1. **Builder ID**: Obtain from admin (created via POST /api/v1/builders)
2. **Private Key**: Generate Ed25519 keypair, provide public key to admin
3. **Server URL**: Crystal Forge server API endpoint
4. **Polling Interval**: Recommended 30 seconds

### Keypair Generation

```bash
# Generate Ed25519 keypair
openssl genpkey -algorithm ED25519 -out builder.key
openssl pkey -in builder.key -pubout -out builder.pub

# Extract base64-encoded public key for registration
openssl pkey -in builder.pub -pubin -outform DER | tail -c +13 | base64
```

### Configuration

```toml
[builder]
builder_id = "uuid-from-admin"
private_key_path = "/path/to/builder.key"
server_url = "https://crystal-forge.example.com"
poll_interval_seconds = 30
max_concurrent_jobs = 2
```

### Builder Polling Loop (Pseudocode)

```rust
loop {
    // 1. Send heartbeat with metrics
    send_heartbeat(builder_id, metrics);
    
    // 2. Check concurrent job limit
    if active_jobs.len() >= max_concurrent_jobs {
        sleep(poll_interval);
        continue;
    }
    
    // 3. Poll for next job
    if let Some(job) = poll_next_job(builder_id) {
        // 4. Execute build in background
        spawn_build_job(job, |status, logs| {
            // 5. Stream logs during build
            append_logs(builder_id, job.id, logs);
            
            // 6. Report completion or failure
            match status {
                Success => complete_job(builder_id, job.id),
                Failed(err) => fail_job(builder_id, job.id, err),
            }
        });
    }
    
    sleep(poll_interval);
}
```

> **Status:** The manual `openssl` keypair steps above are the generic procedure from the source. The builder security architecture describes `cf-keygen` generating the key at `/var/lib/crystal-forge/builder-api.key` on first start (see [builder-credential-boundary-key-management-and-audit-logging.md](../security/builder-credential-boundary-key-management-and-audit-logging.md)); the NixOS module invokes `cf-keygen` in `modules/nixos/crystal-forge/default.nix`.

## Troubleshooting

### Builder Not Receiving Jobs

1. **Check builder status**: `GET /api/v1/builders/:id`
   - Status should be "active"
   - `last_heartbeat_at` should be recent

2. **Check environment assignments**: 
   - Verify builder has correct environment assignments
   - Or zero assignments for wildcard behavior

3. **Check concurrent job limit**:
   - Query active jobs: `SELECT COUNT(*) FROM build_jobs WHERE builder_id = 'uuid' AND status = 'building'`
   - Compare to `max_concurrent_jobs`

4. **Check job queue**:
   - Verify jobs exist: `SELECT * FROM build_jobs WHERE status = 'queued'`
   - Check environment_id matches builder assignments

### Authentication Failures

1. **Verify signature generation**: Ensure signing canonical payload bytes exactly as `METHOD\nPATH\nTIMESTAMP\nRAW_BODY_BYTES`
2. **Check builder status**: Only "active" builders can authenticate
3. **Verify public key**: Ensure public key in database matches private key
4. **Check headers**: `X-Builder-ID`, `X-Signature`, and `X-Timestamp` must be present
5. **Check timestamp freshness**: Request timestamp must be within +/- 5 minutes of server time

### Jobs Not Retrying

1. **Check retry count**: `SELECT retry_count, max_retries FROM build_jobs WHERE id = 'uuid'`
2. **Verify status**: Should be "queued" if retrying, "failed" if exceeded max

## Related concepts

* [Multi-Builder API architecture, scheduling, and environment assignment](../builders/builder-architecture-and-job-scheduling.md) - Describes the multi-builder architecture, environment assignment (wildcard and specific builders), heartbeat and offline detection, query performance, the migration from direct database access, and future enhancements.
* [Builder API: authentication and admin endpoints](../api/builder-api-authentication-and-admin-endpoints.md) - Documents the builder API signature authentication headers and replay window, and the admin endpoints that create, list, update, deactivate, re-key, assign environments to, and read metrics for builders.
* [Builder filesystem layout, firewall rules, and network-constrained configuration](builder-network-and-filesystem-requirements.md) - Gives the builder host filesystem layout and cleanup guarantees, the firewall rules required per execution strategy, the server inbound rules, and example configurations for maximum isolation and for colocated deployments.
