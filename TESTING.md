# TASK-140 Testing Guide

## Overview

This document provides testing instructions for the Multi-Builder API Support feature.

## Prerequisites

- PostgreSQL running (via `db-only up` or `server-stack up`)
- Crystal Forge server running
- Nix development environment

## Quick Start

### 1. Seed Demo Data

```bash
# From the repository root
./scripts/seed_builders.sh
```

This creates 3 demo builders:
- `demo-builder-primary` (active, 16 cores, 32GB, 4 jobs)
- `demo-builder-secondary` (active, 8 cores, 16GB, 2 jobs)
- `demo-builder-offline` (offline, 4 cores, 8GB, 1 job)

Plus 24 hours of metrics history for each builder.

### 2. Access the UI

Navigate to: `http://localhost:8080/builders` (or your configured port)

## UI Testing Checklist

### Builders List View

- [ ] Verify 3 demo builders are displayed
- [ ] Check status badges show correct colors (green=active, red=offline)
- [ ] Verify resource limits are displayed correctly
- [ ] Check heartbeat shows relative time (e.g., "30s ago")
- [ ] Verify environment count shows (or "All (wildcard)")
- [ ] Click "Edit" button on a card - modal should open

### Add Builder Modal

- [ ] Click "➕ Add Builder" button
- [ ] Enter builder name (e.g., "test-builder-01")
- [ ] Click "🔑 Generate Keypair" button
- [ ] Verify public key appears in text area
- [ ] Click "Show" next to Private Key
- [ ] Verify private key appears (hex-encoded, 64 characters)
- [ ] Enter resource limits:
  - Max CPU Cores: 8
  - Max Memory (MB): 16384
  - Max Concurrent Jobs: 2
- [ ] Select environments (checkbox list)
- [ ] Click "Create Builder"
- [ ] Verify modal closes and builder appears in list

### Edit Builder Modal

- [ ] Click "Edit" on any builder card
- [ ] Verify form pre-populates with current values
- [ ] Change builder name
- [ ] Change status dropdown
- [ ] Modify resource limits
- [ ] Toggle environment assignments
- [ ] Click "Save Changes"
- [ ] Verify changes are reflected in the list

### Metrics Dashboard

- [ ] Click "Metrics" tab
- [ ] Verify metrics cards appear for each builder
- [ ] Check CPU usage gauge displays percentage
- [ ] Check memory usage gauge shows MB/GB
- [ ] Verify system stats show (if available)
- [ ] Check "Last updated" timestamp shows relative time

### Deactivate Builder

- [ ] Click "Edit" on a builder
- [ ] Click red "Deactivate Builder" button at bottom left
- [ ] Verify builder status changes to inactive/offline
- [ ] Verify builder disappears or shows different status

## API Testing

### List Builders

```bash
curl http://localhost:3445/api/v1/builders | jq
```

Expected: Array of builders with summary info

### Get Builder Details

```bash
BUILDER_ID="aaaaaaaa-1111-4111-8111-111111111111"
curl http://localhost:3445/api/v1/builders/$BUILDER_ID | jq
```

Expected: Full builder details with environment assignments

### Get Builder Metrics

```bash
BUILDER_ID="aaaaaaaa-1111-4111-8111-111111111111"
curl http://localhost:3445/api/v1/builders/$BUILDER_ID/metrics | jq
```

Expected: Array of metrics (last 24 hours from seed script)

### Create Builder (requires admin auth)

```bash
curl -X POST http://localhost:3445/api/v1/builders \
  -H "Content-Type: application/json" \
  -H "Cookie: auth_token=YOUR_TOKEN" \
  -d '{
    "name": "api-test-builder",
    "public_key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    "max_cpu_cores": 4,
    "max_memory_mb": 8192,
    "max_concurrent_jobs": 1,
    "environment_ids": []
  }' | jq
```

Expected: Created builder details with UUID

## Backend Testing

### Run Unit Tests

```bash
nix develop -c cargo test --lib builders
```

### Run Clippy

```bash
nix develop -c cargo clippy --all-targets -- -D warnings
```

### Check Formatting

```bash
nix develop -c cargo fmt -- --check
```

## Database Verification

### Check Tables

```sql
-- Count builders
SELECT COUNT(*) FROM builders;

-- Check metrics
SELECT builder_id, timestamp, cpu_usage_percent, memory_usage_mb 
FROM builder_metrics 
ORDER BY timestamp DESC 
LIMIT 10;

-- Check environment assignments
SELECT b.name, e.name as environment
FROM builders b
JOIN builder_environment_assignments bea ON b.id = bea.builder_id
JOIN environments e ON bea.environment_id = e.id;
```

## Known Issues / Limitations

1. **Keypair Generation**: Current implementation uses browser crypto to generate random bytes for both public and private keys independently. This is NOT cryptographically correct for Ed25519 (public key should be derived from private key). For production use, replace with proper Ed25519 library or server-side keypair generation endpoint.

2. **Builder API Mode**: The builder binary has API mode infrastructure but job execution is not yet integrated (Phase 6 TODO). It will poll for jobs and send heartbeats, but fail jobs with "not implemented" message.

3. **Real-time Updates**: UI does not auto-refresh. Manual refresh required to see updated metrics.

## Success Criteria

### Backend (Phases 1-5)
- ✅ All API endpoints functional
- ✅ Authentication working
- ✅ Job queue operations
- ✅ Environment filtering
- ✅ Retry logic

### Builder Binary (Phase 6)
- ✅ API client implemented
- ✅ Metrics collection working
- ✅ Dual mode (DB/API) support
- ⚠️ Job execution integration pending

### Frontend (Phases 8-9)
- ✅ Builders list with real data
- ✅ Add/edit/deactivate modals
- ✅ Environment assignment
- ✅ Resource configuration
- ✅ Metrics dashboard
- ✅ Keypair generation (with caveat)

## Test Results

Please report test results and any issues found during testing.

Date: _________
Tester: _________
Environment: _________

| Test Area | Status | Notes |
|-----------|--------|-------|
| Seed script | | |
| Builders list | | |
| Add modal | | |
| Edit modal | | |
| Deactivate | | |
| Metrics view | | |
| API endpoints | | |
| Database state | | |

## Feedback

UI/UX feedback:
- 
-
-

Bug reports:
-
-
-

Feature requests:
-
-
-
