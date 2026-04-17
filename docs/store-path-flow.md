# Crystal Forge Store Path Flow

This diagram shows how Crystal Forge evaluates flake commits, tracks expected store paths per system, builds configurations in parallel, and keeps agents up-to-date.

## The Complete Flow

```mermaid
flowchart TB
    subgraph "1. Flake Commit Arrives"
        COMMIT[New Flake Commit SHA]
    end

    subgraph "2. Parallel Evaluation (per nixosConfiguration)"
        COMMIT --> EVAL1[Eval hostname1<br/>nix eval --dry-run]
        COMMIT --> EVAL2[Eval hostname2<br/>nix eval --dry-run]
        COMMIT --> EVAL3[Eval hostname3<br/>nix eval --dry-run]
        
        EVAL1 --> STORE1[Store Path:<br/>/nix/store/abc123...]
        EVAL2 --> STORE2[Store Path:<br/>/nix/store/def456...]
        EVAL3 --> STORE3[Store Path:<br/>/nix/store/ghi789...]
        
        STORE1 --> DB1[(DB: Save<br/>hostname1 → abc123<br/>commit → SHA)]
        STORE2 --> DB2[(DB: Save<br/>hostname2 → def456<br/>commit → SHA)]
        STORE3 --> DB3[(DB: Save<br/>hostname3 → ghi789<br/>commit → SHA)]
    end

    subgraph "3. Build Queue (LIFO - Newest First)"
        DB1 --> QUEUE[Build Queue]
        DB2 --> QUEUE
        DB3 --> QUEUE
        
        QUEUE --> |Newest commits<br/>jump to front| QUEUENOTE[📋 LIFO Order:<br/>Latest commit builds first]
    end

    subgraph "4. Build & Cache (Parallel, Independent Per System)"
        QUEUE --> BUILD1[Build hostname1]
        QUEUE --> BUILD2[Build hostname2]
        QUEUE --> BUILD3[Build hostname3]
        
        BUILD1 --> CACHE1[Push to Cache<br/>hostname1/abc123]
        BUILD2 --> CACHE2[Push to Cache<br/>hostname2/def456]
        BUILD3 --> CACHE3[Push to Cache<br/>hostname3/ghi789]
        
        CACHE1 --> DBDONE1[(DB: Mark<br/>hostname1/abc123<br/>CACHED)]
        CACHE2 --> DBDONE2[(DB: Mark<br/>hostname2/def456<br/>CACHED)]
        CACHE3 --> DBDONE3[(DB: Mark<br/>hostname3/ghi789<br/>CACHED)]
    end

    subgraph "5. Agent Polling & Update"
        AGENT[Agent<br/>hostname1]
        
        AGENT --> |Heartbeat:<br/>current store path| HEARTBEAT[CF: Compare<br/>Agent Path vs DB]
        
        HEARTBEAT --> UPTODATE{State?}
        UPTODATE --> |Match| STATE1[✅ UP-TO-DATE<br/>Agent path = DB path]
        UPTODATE --> |Mismatch| STATE2[⚠️ BEHIND<br/>DB has newer path]
        UPTODATE --> |Not Found| STATE3[❓ UNKNOWN<br/>Path not in DB]
        
        AGENT --> |Poll CF| POLL[Check for<br/>new builds]
        POLL --> DBDONE1
        POLL --> |New cached build| PULL[Pull from Cache]
        PULL --> ACTIVATE[Agent activates<br/>new config]
        ACTIVATE --> NEWSTATE[Agent now:<br/>/nix/store/abc123]
    end

    style COMMIT fill:#e1f5ff
    style EVAL1 fill:#fff4e1
    style EVAL2 fill:#fff4e1
    style EVAL3 fill:#fff4e1
    style QUEUE fill:#ffe1f5
    style BUILD1 fill:#e1ffe1
    style BUILD2 fill:#e1ffe1
    style BUILD3 fill:#e1ffe1
    style STATE1 fill:#90EE90
    style STATE2 fill:#FFD700
    style STATE3 fill:#FFB6C1
```

## Key Points for Dumb Interns

### Parallel Operations
- **Each nixosConfiguration (hostname1, hostname2, etc.) flows INDEPENDENTLY**
- hostname1 can be building while hostname2 is still evaluating
- hostname3 can be caching while hostname1 is queued
- NO waiting for all evals to finish before builds start

### LIFO Build Queue
- **Newest commit's configs jump to the FRONT of the queue**
- If commit A arrives, then commit B arrives:
  - Commit B's configs build BEFORE commit A's remaining configs
- Why? Because we want latest changes deployed fastest

### System States (How We Know What's Happening)

| State | Meaning | Agent Store Path | DB Store Path |
|-------|---------|------------------|---------------|
| ✅ **UP-TO-DATE** | Agent running latest | `/nix/store/abc123...` | `/nix/store/abc123...` (MATCH) |
| ⚠️ **BEHIND** | Agent running old config | `/nix/store/old111...` | `/nix/store/abc123...` (NEWER) |
| ❓ **UNKNOWN** | Agent path not tracked | `/nix/store/xyz999...` | NOT FOUND in DB |

### The Loop
1. **Commit arrives** → CF starts eval for each hostname
2. **Eval finishes** → Store path saved to DB immediately
3. **Queue entry** → System added to build queue (LIFO)
4. **Build starts** → Independent of other systems still evaluating
5. **Build done** → Push to cache, mark in DB as CACHED
6. **Agent polls** → Discovers new build available
7. **Agent pulls** → Downloads from cache, activates config
8. **Agent heartbeats** → Reports new store path, CF marks UP-TO-DATE

### Why This Design?
- **Parallel eval** = Fast feedback on which configs are changing
- **Independent pipeline** = No blocking, max throughput
- **LIFO queue** = Latest changes deploy first
- **Store path in DB** = Single source of truth for "what should be running"
- **Agent polling** = Agent pulls updates when ready (no push complexity)
