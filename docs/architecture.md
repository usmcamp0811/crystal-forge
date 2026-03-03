# ADR-000: Crystal Forge Architecture Overview

## Status

Accepted

## Context

Crystal Forge provides compliance monitoring and build coordination for NixOS systems in regulated environments. The architecture must support horizontal scaling, cryptographic verification, and integration with existing compliance workflows.

## Decision

### Core Components

```mermaid
flowchart LR
    A[Agent<br/>NixOS hosts]

    subgraph "Core Infrastructure"
        S[Server<br/>API/Coord]
        B[Builder<br/>Eval/CVE scan]
        P[PostgreSQL<br/>shared state]
        G[Grafana<br/>dashboards/alerts]
    end

    A -->|HTTP POST<br/>signed state| S
    B --> P
    S --> P
    P --> G

    %% Styling to make boxes more rectangular
    classDef default fill:#f9f9f9,stroke:#333,stroke-width:2px,color:#000
```

#### Agent (Rust)

- **Location**: Runs on each monitored NixOS system
- **Responsibilities**:
  - Monitor system configuration changes via inotify
  - Collect system fingerprints (hardware, software, security status)
  - Send Ed25519-signed state reports to server
  - Heartbeat vs. state change intelligence
- **Interfaces**: HTTP POST to server `/agent/heartbeat` and `/agent/state`

#### Server (Rust)

- **Location**: Central coordination node(s)
- **Responsibilities**:
  - Receive and verify agent reports
  - Process Git webhooks for configuration updates
  - Coordinate build requests
  - Provide API for compliance queries
- **Interfaces**:
  - HTTP API for agents
  - Webhook endpoints for Git repositories
  - Database read/write operations

#### Builder (Rust)

- **Location**: Build coordination node(s)
- **Responsibilities**:
  - Evaluate NixOS flakes on demand
  - Build derivations for CVE scanning
  - Run vulnix for vulnerability assessment
  - Track configuration drift (current vs. latest)
- **Interfaces**:
  - Database coordination with server
  - Nix evaluation engine integration
  - vulnix CVE scanning integration

### Data Flows

#### 1. State Monitoring Flow

```
NixOS System → Agent → Server → PostgreSQL → Grafana
```

Agent detects configuration change → Signs state report → Server validates signature → Stores compliance data → Grafana displays/alerts

#### 2. CVE Scanning Flow

```
Git Webhook → Server → Builder → vulnix → PostgreSQL → Grafana
```

Configuration update → Server triggers build → Builder evaluates flake → Runs CVE scan → Stores vulnerability data → Compliance dashboard updates

#### 3. Drift Detection Flow

```
Agent State + Builder Evaluation → Server Comparison → Compliance Alert
```

Current system state compared against latest evaluated configuration to detect unauthorized changes.

### Event-Driven Queue Architecture

Crystal Forge uses an event-driven architecture for both evaluation and build queues, replacing polling-based approaches with immediate notifications.

#### Queue Notification System

The `QueueNotifier` provides bounded event channels using Tokio MPSC:

```rust
pub struct QueueNotifier {
    eval_tx: mpsc::Sender<()>,   // channel(1), coalesced wakeups
    eval_rx: Arc<Mutex<mpsc::Receiver<()>>>,
    build_tx: mpsc::Sender<()>,  // channel(1), coalesced wakeups
    build_rx: Arc<Mutex<mpsc::Receiver<()>>>,
}
```

**Key Benefits**:
- **Zero-latency triggering**: Work starts immediately when commits/jobs arrive
- **Bounded memory**: channel capacity is 1 and duplicate wakeups are coalesced
- **Idle efficiency**: No CPU cycles wasted polling empty queues
- **Fallback safety**: Periodic ticks catch any missed notifications

#### Eval Queue Flow

```
Commit Insert → notify_eval_queue() → Eval Loop Wakes → Process Pending
                                    ↓
                        (fallback: 60s ticker)
```

**Trigger Points**:
1. Flake polling discovers new commits
2. Webhook receives push notification
3. Manual commit insertion via API

**Processing Loop**:
```rust
loop {
    process_pending_commits(&pool, &cf_state, &queue_notifier).await;

    tokio::select! {
        _ = ticker.tick() => { /* fallback: every 60s */ }
        _ = queue_notifier.wait_for_eval_work() => { /* immediate */ }
    }
}
```

#### Build Queue Flow

```
Eval Complete → create_build_jobs() → notify_build_queue() → Build Workers Wake
                                                           ↓
                                        (NOT YET IMPLEMENTED: workers still poll 5s)
```

**Current State**:
- Server-side build job creation triggers notification
- Build workers (separate processes) still poll every 5s
- Future: PostgreSQL LISTEN/NOTIFY or unified process model

#### Notification Guarantees

**Fire-and-Forget Semantics**:
- Notifications never block the sender
- Dropped receivers (server shutdown) are silently ignored
- Multiple notifications coalesce into one pending wakeup

**FIFO Ordering**:
- MPSC channels guarantee insertion order
- First commit inserted → first evaluation started
- First build job created → first worker claims it

**Fallback Polling**:
- Eval loop: 60s ticker (catches DB corruption, missed signals)
- Build workers: 5s ticker (until event-driven build implemented)

### Key Architectural Decisions

1. **Shared PostgreSQL**: Enables horizontal scaling of servers and builders
2. **Ed25519 signatures**: Cryptographic verification of all agent communications
3. **Rust implementation**: Memory safety and performance for security-critical deployment
4. **Event-driven queues**: Immediate processing with FIFO guarantees and fallback polling
5. **Flake-native**: Direct integration with modern Nix ecosystem

### Observability Points

1. **Agent health monitoring**: Heartbeat frequency, signature validation success rate
2. **Build coordination metrics**: Evaluation times, CVE scan duration, queue depth
3. **Compliance metrics**: Systems in drift, CVE exposure levels, STIG compliance rates
4. **Database performance**: Query times, connection counts, replication lag

## Consequences

**Positive**:

- Horizontal scaling through shared database
- Strong cryptographic security model
- Integration with existing monitoring infrastructure (Grafana)
- Memory-safe implementation reduces attack surface

**Negative**:

- PostgreSQL becomes single point of failure (mitigated by standard HA practices)
- Rust learning curve for contributors
- Initial dependency on Grafana for user interface

## Future Evolution

- Custom web frontend to replace Grafana dashboards
- Agent deployment capabilities for configuration management
- Support for additional CVE scanning tools beyond vulnix
