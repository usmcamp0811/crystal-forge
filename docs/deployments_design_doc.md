# Crystal Forge Agent Deployment Design Document

## Overview

This document outlines the design for adding bidirectional deployment capabilities to Crystal Forge, enabling the server to instruct agents which NixOS configuration they should be running.

## Current State

Crystal Forge currently operates as a monitoring system where:

- Agents POST their system state to the server via HTTP
- Server responds with HTTP 200/OK
- All communication is one-way (agent → server)
- Agents are authenticated via Ed25519 signatures
- Server tracks system state but cannot influence it

## Proposed Architecture

### Communication Model

Extend the existing agent POST workflow to include deployment instructions in the HTTP response:

```
Agent POST /agent/state → Server Response with deployment instructions
```

This maintains the existing communication pattern while adding server-to-agent capability.

### Database Schema Changes

#### Systems Table Extensions

```sql
ALTER TABLE systems
  ADD COLUMN desired_target TEXT,
  ADD COLUMN deployment_policy TEXT DEFAULT 'manual'
    CHECK (deployment_policy IN ('manual', 'auto_latest', 'pinned')),
  ADD COLUMN server_public_key TEXT;
```

#### Derivations Table Extension

```sql
ALTER TABLE derivations
  ADD COLUMN crystal_forge_enabled BOOLEAN DEFAULT FALSE;
```

#### System States Change Reason Extension

```sql
-- Add new change reason for agent-initiated deployments
-- Existing: 'startup', 'config_change', 'state_delta'
-- New: 'cf_deployment'
```

### Deployment Policies

**Per-system deployment policies:**

1. **`manual`** (default): Admin explicitly sets `desired_target`
2. **`auto_latest`**: Always deploy latest successful derivation for system's flake
3. **`pinned`**: Stay on current derivation until manually changed

Future enhancement: Group-based policies for fleet management.

### Security Model

#### Server Authentication to Agents

- Server signs deployment commands with Ed25519 private key
- Agents verify deployment commands using server's public key
- Public key distributed via agent NixOS configuration

#### Deployment Command Structure

```json
{
  "deployment": {
    "derivation_path": "/nix/store/abc123-nixos-system-hostname",
    "signature": "base64-encoded-ed25519-signature-of-derivation-path",
    "cache_url": "https://cache.company.com",
    "timestamp": "2025-09-21T10:30:00Z"
  }
}
```

### Crystal Forge Assertion

#### Build-time Validation

- After derivation build, use `nix repl` to inspect configuration
- Assert `services.crystal-forge.client.enable = true`
- Set `derivations.crystal_forge_enabled = true` only if assertion passes
- Block deployment of derivations where `crystal_forge_enabled = false`

This prevents agents from deploying configurations that would disconnect them from Crystal Forge.

### Cache Integration

#### Prerequisites

- Binary cache (S3, Attic, etc.) must be configured
- Derivations must be pushed to cache before becoming deployable
- Integration with existing `cache_push_jobs` table

#### Deployment Flow

1. Derivation builds successfully
2. Derivation pushed to cache (`cache-pushed` status)
3. Crystal Forge assertion validates agent enablement
4. Derivation becomes available for deployment
5. Agent receives deployment instruction
6. Agent pulls from cache and deploys

#### Fallback Strategy

- Configurable: allow local builds if cache miss occurs
- Default: require cache availability for deployments
- Fail deployment if neither cache nor local build allowed

### Agent Deployment Process

#### Current Agent POST Response

```json
{ "status": "ok" }
```

#### Enhanced Agent POST Response

```json
{
  "status": "ok",
  "deployment": {
    "derivation_path": "/nix/store/abc123-nixos-system-hostname",
    "signature": "base64-signature",
    "cache_url": "https://cache.company.com",
    "timestamp": "2025-09-21T10:30:00Z"
  }
}
```

#### Agent Deployment Logic

1. Agent POSTs current state to server
2. Server responds with deployment instructions (if any)
3. Agent validates server signature
4. Agent checks if current derivation != desired derivation
5. If different:
   - Agent pulls derivation from cache
   - Agent executes `nixos-rebuild switch` equivalent
   - Agent reports new state on next heartbeat with `change_reason = 'cf_deployment'`

### Hostname to NixOS Configuration Mapping

Leverage existing Crystal Forge auto-discovery:

- Server already maps hostname → nixosConfiguration output
- No additional configuration required
- Maintains compatibility with any flake containing nixosConfigurations

### Error Handling & Safety

#### Deployment Failures

- Agent attempts deployment
- If failure occurs, agent remains on previous configuration
- Agent reports failure in next state POST
- No automatic retry (admin intervention required)

#### Network Failures

- Agent deployment process is idempotent
- If agent goes offline during deployment, next heartbeat will retry
- Server tracks intended vs actual state via system_states table

#### Rollback Strategy

- Manual rollback only (for initial implementation)
- Admin sets `desired_target` to previous known-good derivation
- Future enhancement: automatic rollback on deployment failure

### Implementation Phases

#### Phase 1: Core Deployment (Initial Implementation)

- Database schema changes
- Server-side deployment response logic
- Agent-side deployment execution
- Manual deployment policy only
- Crystal Forge assertion validation

#### Phase 2: Enhanced Policies

- `auto_latest` deployment policy
- `pinned` deployment policy
- Deployment scheduling capabilities

#### Phase 3: Fleet Management

- Group-based deployment policies
- Staged deployments
- Advanced rollback strategies

### Constraints & Requirements

#### Security Requirements

- All deployment commands cryptographically signed
- Crystal Forge assertion prevents agent disconnection
- Maintain existing Ed25519 authentication model

#### Compatibility Requirements

- Work with any flake containing nixosConfigurations
- No changes required to existing flake structures
- Backward compatible with current agent behavior

#### Operational Requirements

- Binary cache must be available for deployments
- Agents must have network access to cache
- Server must have signing key for agent authentication

### Future Considerations

#### Group-Based Policies

Design supports future enhancement to group-based deployment policies without schema changes. Groups could be implemented as separate table referencing systems.

#### Deployment Scheduling

Current design supports immediate deployments. Future enhancement could add scheduling by extending the deployment response structure.

#### Advanced Monitoring

Integration with existing Grafana dashboards to track deployment success rates, timing, and failures.

## Risks & Mitigations

| Risk                                               | Impact | Mitigation                                                     |
| -------------------------------------------------- | ------ | -------------------------------------------------------------- |
| Agent deployment failure leaves system unreachable | High   | Crystal Forge assertion prevents configs that disable agent    |
| Cache unavailability blocks deployments            | Medium | Configurable fallback to local builds                          |
| Deployment command replay attacks                  | Medium | Include timestamp in signed payload, agent validates freshness |
| Mass deployment failures                           | High   | Start with manual policy, add staged deployments in Phase 2    |

## Success Criteria

1. Agents can receive and execute deployment commands from server
2. Deployments only occur for configurations that maintain Crystal Forge connectivity
3. All deployment commands are cryptographically verified
4. System maintains backward compatibility with existing agent behavior
5. Failed deployments do not leave systems in unrecoverable state
