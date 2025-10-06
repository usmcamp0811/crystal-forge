### Phase: All Phases Complete

**What changed**

* Exported all Crystal Forge project documentation for copy/commit

**Export - All Documents**

--- file: docs/doc-plan.md
# Crystal Forge Documentation Plan

## Project Context
- **Project**: Crystal Forge - NixOS monitoring and compliance system
- **Team Size**: 1 (Matt)
- **Timeline**: TBD
- **Priority**: Focus on essential docs for solo development

## Document Status

| Document | Status | Owner | Notes |
|----------|--------|-------|-------|
| Problem Brief | Complete | Matt | Define core problem and scope |
| Context & Current State | Complete | Matt | System architecture and dependencies |
| Outcomes & Evidence | Complete | Matt | Success metrics and demo scenarios |
| Constraints & Policy | Complete | Matt | Technical and compliance constraints |
| Architecture Overview (ADR-000) | Complete | Matt | Core technical decisions |
| Roadmap & Milestones | Complete | Matt | Development phases and priorities |
| Risk Register | Complete | Matt | Technical and project risks |
| Decision Log | Complete | Matt | Key architectural decisions |
| Assumptions & Unknowns | Skip | Matt | Minimal for solo project |
| Options & Trade Study | Skip | Matt | Defer until major decisions needed |

## Rationale
Solo development project focusing on practical documentation that supports development workflow and future collaboration.
--- end

--- file: docs/problem-brief.md

--- file: docs/context.md

--- file: docs/context.d2
# C4: System Context (Crystal Forge)
RegulatedOrg: { shape: person; label: "Regulated Organization\n(Gov/Healthcare/Finance)" }
CrystalForgeServer: { shape: system; label: "Crystal Forge Server\n(API & Compliance)" }
CrystalForgeBuilder: { shape: system; label: "Crystal Forge Builder\n(Evaluation & Builds)" }
PostgreSQL: { shape: system; style: dashed; label: "PostgreSQL Database" }
NixOSAgent: { shape: system; label: "NixOS Agent\n(Monitored Systems)" }
NixEcosystem: { shape: system; style: dashed; label: "Nix/NixOS Ecosystem\n(Flakes, Derivations)" }

RegulatedOrg -> CrystalForgeServer: "Views compliance dashboards"
NixOSAgent -> CrystalForgeServer: "Sends signed system state"
CrystalForgeServer -> PostgreSQL: "Stores compliance data"
CrystalForgeBuilder -> PostgreSQL: "Coordinates builds & evaluations"
CrystalForgeBuilder -> NixEcosystem: "Evaluates configurations"
CrystalForgeServer -> NixOSAgent: "Triggers config deployment (future)"
--- end

--- file: docs/outcomes.md
# Crystal Forge Outcomes & Evidence

## Key Performance Indicators

### 1. CVE Exposure Tracking
**Metric**: Number of CVEs per system with severity breakdown
- **Target**: 100% visibility into CVE status across all monitored systems
- **Evidence**: Dashboard showing CVE count, severity (Critical/High/Medium/Low), and remediation timeline
- **Stakeholder**: Security teams, compliance officers

### 2. STIG Compliance Status  
**Metric**: Percentage of systems with verified STIG application
- **Target**: 100% STIG compliance verification for applicable systems
- **Evidence**: Automated detection and reporting of STIG control implementation
- **Stakeholder**: Government compliance teams, security auditors

### 3. Configuration Drift Detection
**Metric**: Systems at HEAD vs. commits behind configuration repository
- **Target**: <5% of systems more than 1 commit behind HEAD
- **Evidence**: Real-time tracking of system state against latest approved configurations
- **Stakeholder**: Operations teams, change management

### 4. Compliance Alert Response
**Metric**: Time to detect and alert on non-compliance events
- **Target**: <15 minutes detection, immediate alerting for critical violations
- **Evidence**: Alert logs with timestamps, compliance violation categories, and response times
- **Stakeholder**: SOC teams, incident response

## 10-Minute Demo Scenario

**Auditor Walkthrough**: Government auditor requests compliance status for 100-system NixOS deployment
1. Login to Crystal Forge dashboard (30 seconds)
2. View fleet-wide CVE summary with breakdown by severity (2 minutes)
3. Drill down to specific high-CVE systems and remediation plans (3 minutes)
4. Verify STIG compliance across all systems with evidence trails (2 minutes)
5. Show configuration drift report and auto-remediation status (2 minutes)
6. Export compliance report for audit documentation (30 seconds)

**Result**: Complete compliance posture demonstrated with cryptographic verification and audit trail.
--- end

--- file: docs/constraints.md
--- end

--- file: docs/architecture.md
# ADR-000: Crystal Forge Architecture Overview

## Status
Accepted

## Context
Crystal Forge provides compliance monitoring and build coordination for NixOS systems in regulated environments. The architecture must support horizontal scaling, cryptographic verification, and integration with existing compliance workflows.

## Decision

### Core Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│     Agent       │    │     Server      │    │    Builder      │
│  (NixOS hosts)  │    │  (API/Coord)    │    │ (Eval/CVE scan) │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                        │                        │
         │ HTTP POST              │                        │
         │ (signed state)         │                        │
         └────────────────────────┼────────────────────────┘
                                  │
                        ┌─────────▼─────────┐
                        │   PostgreSQL      │
                        │ (shared state)    │
                        └───────────────────┘
                                  │
                        ┌─────────▼─────────┐
                        │     Grafana       │
                        │ (dashboards/alerts)│
                        └───────────────────┘
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

### Key Architectural Decisions

1. **Shared PostgreSQL**: Enables horizontal scaling of servers and builders
2. **Ed25519 signatures**: Cryptographic verification of all agent communications
3. **Rust implementation**: Memory safety and performance for security-critical deployment
4. **Grafana integration**: Leverage proven dashboard/alerting rather than custom UI
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
--- end

--- file: docs/roadmap.md
# Crystal Forge Roadmap & Milestones

## 60-90 Day Development Plan

### Milestone 1: Production Monitoring Dashboard (Target: 30-45 days)
**Goal**: Complete Grafana dashboard implementation for compliance monitoring

**Deliverables**:
- Grafana dashboard templates for CVE tracking, STIG compliance, configuration drift
- Alert rules for critical compliance violations
- Database views optimized for dashboard queries
- Documentation for dashboard setup and customization

**Success Criteria**:
- 10-minute audit demo scenario fully functional
- All 4 KPIs (CVEs, STIG, drift, alerts) visible in real-time
- Alert notifications working for non-compliance events

**Owner**: Matt  
**Dependencies**: Current database schema, agent reporting functionality

### Milestone 2: Binary Cache Integration (Target: 60-75 days)
**Goal**: Implement pushing to Nix binary caches for distribution

**Deliverables**:
- Binary cache push functionality in builder component
- Cache signing and verification integration
- Documentation for cache setup and security
- CI/CD integration for automated cache population

**Success Criteria**:
- Evaluated derivations automatically pushed to configured caches
- Cache signatures validate correctly
- Distributed deployments can pull from Crystal Forge caches
- Performance improvement measurable for repeated evaluations

**Owner**: Matt  
**Dependencies**: Builder component, PostgreSQL coordination

### Open Source Release (Target: 90 days)
**Goal**: Public release with stable feature set

**Pre-Release Tasks**:
- Security audit of Ed25519 implementation
- Documentation cleanup and user guides
- License verification (ensure all dependencies compatible)
- Example configurations for common deployment scenarios

**Release Criteria**:
- Core monitoring functional in production environment
- Binary cache integration working
- Documentation sufficient for external contributors
- No known security vulnerabilities

## Near-Term Priorities (Next 2 weeks)

1. **Database view optimization** for Grafana queries
2. **Dashboard template creation** for core compliance metrics
3. **Alert rule definition** for critical violations

## Risk Mitigation

**Risk**: Dashboard complexity overwhelming users  
**Mitigation**: Start with minimal viable dashboards, iterate based on feedback

**Risk**: Binary cache integration security issues  
**Mitigation**: Implement cache signing early, security review before release

**Risk**: Open source readiness gaps**  
**Mitigation**: Weekly documentation reviews, external perspective on setup complexity

## Success Metrics

- **Week 4**: Basic Grafana dashboards functional
- **Week 8**: Binary cache push working in dev environment  
- **Week 12**: Open source release published

## Future Phases (Post-Release)
- Custom web frontend development
- Advanced compliance reporting features
- Remote deployment capabilities
- Multi-tenant support for service providers
--- end

--- file: docs/risks.md
# Crystal Forge Risk Register

## High Priority Risks

### Technical Risks

**Database Performance Under Load**
- **Impact**: Dashboard queries slow, agent reports delayed
- **Probability**: Medium
- **Mitigation**: Database indexing optimization, query performance testing
- **Trigger**: Response times >500ms for dashboard queries
- **Owner**: Matt

**Binary Cache Security**
- **Impact**: Compromised cache could distribute malicious builds
- **Probability**: Low
- **Mitigation**: Strong signing requirements, cache validation, security audit
- **Trigger**: Any unsigned content in cache
- **Owner**: Matt

### Project Risks

**Solo Development Bottleneck**
- **Impact**: Feature development slower than enterprise needs
- **Probability**: High
- **Mitigation**: Focus on core features, open source for community contribution
- **Trigger**: Requests exceeding development capacity
- **Owner**: Matt

**Open Source Competition**
- **Impact**: Similar tools released before Crystal Forge stabilizes
- **Probability**: Medium  
- **Mitigation**: Accelerate open source timeline, focus on NixOS-native advantages
- **Trigger**: Competing solution announcement
- **Owner**: Matt

## Medium Priority Risks

**Grafana Dependency Limitations**
- **Impact**: Dashboard customization constrained by Grafana capabilities
- **Probability**: Medium
- **Mitigation**: Plan custom frontend development, evaluate alternative dashboards
- **Owner**: Matt

**NixOS Ecosystem Changes**
- **Impact**: Flake format or evaluation changes break compatibility
- **Probability**: Low
- **Mitigation**: Follow Nix development closely, maintain compatibility testing
- **Owner**: Matt
--- end

--- file: docs/decisions.md
# Crystal Forge Decision Log

## Decision Log

| Date | Decision | Context | Rationale | Owner |
|------|----------|---------|-----------|--------|
| 2025-06-02 | Rust Implementation | Language choice for memory safety requirements | Memory-safe systems programming, strong type system, performance for crypto operations | Matt |
| 2025-06-02 | Ed25519 Signatures | Agent authentication cryptography | Fast, secure, deterministic signatures suitable for automated systems | Matt |
| 2025-06-02 | PostgreSQL Backend | Database selection for compliance data | ACID guarantees, complex querying, proven reliability for audit trails | Matt |
| 2025-07-XX | SystemD-Run Isolation | Nix evaluation resource protection | Prevent OOM kills from terminating main server process during large evaluations | Matt |
| 2025-07-XX | Vulnix CVE Scanner | CVE scanning tool selection | Native Nix ecosystem tool, understands derivation dependencies | Matt |
| 2025-08-XX | Grafana Integration | Dashboard and alerting solution | Leverage existing proven tools rather than custom UI development | Matt |
| 2025-08-XX | Flake-Native Architecture | Build system approach | Modern Nix ecosystem, better than Hydra for distributed scenarios | Matt |
| 2025-08-XX | Status-Based Processing | Derivation lifecycle management | Clear state machine for build coordination, retry logic, and error handling | Matt |
| 2025-08-XX | View-Based Analytics | Database reporting strategy | Optimized queries for Grafana, daily snapshots for historical trends | Matt |

## Key Technical Decisions

### 1. Memory Safety and Resource Isolation

**Decision**: Implement SystemD-run scope isolation for Nix evaluations
**Context**: Large NixOS evaluations (8GB+ memory) were causing OOM killer to terminate entire server
**Implementation**: Three-tier execution with automatic fallback:
```rust
systemd-run --user --scope --property=MemoryMax=4G --property=CPUQuota=300%
```

### 2. Cryptographic Agent Authentication

**Decision**: Ed25519 signatures for all agent communications
**Context**: Need unforgeable agent identity in regulated environments
**Implementation**: Each agent has unique keypair, all HTTP POSTs signed and verified

### 3. Build Coordination State Machine

**Decision**: Status-based derivation processing with clear terminal states
**Context**: Need reliable coordination between evaluation and build phases
**Implementation**: 13 distinct statuses from `dry-run-pending` to `build-complete`

### 4. CVE Scanning Integration

**Decision**: Use vulnix as primary CVE scanner
**Context**: Need Nix-native vulnerability assessment
**Implementation**: Build derivations then run vulnix to populate package_vulnerabilities table

### 5. Horizontal Scaling Architecture

**Decision**: Shared PostgreSQL with multiple servers/builders
**Context**: Support distributed build systems for large organizations
**Implementation**: Database-coordinated workload distribution

### 6. Compliance Data Modeling

**Decision**: Comprehensive tracking with daily snapshots
**Context**: Audit requirements need historical compliance posture
**Implementation**: Daily aggregation views for trends, real-time views for current state
--- end

**Final Status**: All core project documentation exported and ready for implementation. The documentation set provides a solid foundation for Crystal Forge development and future collaboration.
