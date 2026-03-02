# Frontend Views Specification

This document describes each UI view in Crystal Forge. It's written for developers who need to understand what each view does, what data it shows, and how users interact with it.

**Assumption:** You have basic knowledge of React/Dioxus concepts (components, state, routing).

---

## Navigation Structure

The app uses a **sidebar navigation** pattern:

```
┌────────────────────────────────────────────────┐
│  ┌──────┐                                     │
│  │ Logo │  Crystal Forge                       │
│  └──────┘                                     │
├────────┬───────────────────────────────────────┤
│        │                                       │
│  Dash  │                                       │
│        │                                       │
│Systems │         Main Content Area              │
│        │                                       │
│ Flakes │         (Changes based on route)       │
│        │                                       │
│Environ │                                       │
│        │                                       │
│ Builds │                                       │
│        │                                       │
│ Admin  │                                       │
│        │                                       │
└────────┴───────────────────────────────────────┘
```

**Route Mapping:**
- `/` → Dashboard
- `/systems` → Systems List
- `/systems/:id` → System Detail
- `/flakes` → Flakes List
- `/environments` → Environments List
- `/builds` → Builds Queue
- `/admin` → Admin Console

---

## Dashboard (`/`)

**Route:** `/`

**Purpose:** Give users a quick overview of their fleet without having to click around.

### What It Shows

1. **Fleet Summary Card**
   - Total systems count
   - Online vs offline breakdown
   - Systems per environment (e.g., "Prod: 5, Dev: 3")

2. **Build Queue Widget**
   - How many builds are pending
   - How many are currently building
   - Recent build activity (last 5 builds)

3. **Flake Timeline Widget**
   - Recent commits across all tracked flakes
   - Shows commit message, author, time
   - Clicking a commit takes you to that flake

4. **Quick Actions**
   - "Deploy New System" button
   - "Sync All Flakes" button
   - Links to common tasks

### Data Flow

```
Frontend                     Backend
   │                           │
   ├─ GET /api/v1/dashboard ─►│
   │                           │
   │◄─── {                   ◄──│
   │      systems: {           │
   │        total: 8,          │
   │        online: 7,         │
   │        offline: 1         │
   │      },                   │
   │      environments: {...},  │
   │      builds: {...},       │
   │      flakes: {...}        │
   │    }                     │
```

### How to Modify

- **Backend:** Modify `handlers/api/dashboard.rs`
- **Frontend:** Modify `views/dashboard.rs`

---

## Systems List (`/systems`)

**Route:** `/systems`

**Purpose:** See all registered systems at a glance, filter them, and navigate to specific systems.

### What It Shows

1. **View Toggle**
   - **Cards View:** Visual cards with system info (default)
   - **Table View:** Compact table for many systems

2. **Filter Controls**
   - **Environment Dropdown:** Filter by prod/staging/dev
   - **Status Dropdown:** All / Online / Offline
   - **Search Box:** Filter by name or hostname

3. **System Cards/Rows**
   Each card shows:
   - System name (bold)
   - Hostname (subtitle)
   - Environment badge (colored)
   - Status indicator (green dot = online, red = offline)
   - Last heartbeat time
   - Currently deployed flake name

### User Interactions

| Action | Result |
|--------|--------|
| Click card | Navigate to System Detail |
| Click environment badge | Filter list to that environment |
| Click "Deploy" | Open deployment modal |
| Click "Add System" | Open add system modal |

### Data Flow

```
Frontend                                    Backend
   │                                            │
   ├─ GET /api/v1/systems?environment=prod ────►│
   │                                            │
   │◄─── { systems: [...] } ───────────────────│
```

**Query Parameters:**
- `?environment=prod` - Filter by environment
- `?status=online` - Filter by status
- `?search=web` - Search name/hostname

### How to Modify

- **Backend:** `handlers/api/systems.rs`, `queries/systems.rs`
- **Frontend:** `views/systems_list.rs`, `systems/adapter.rs`

---

## System Detail (`/systems/:id`)

**Route:** `/systems/:id` (e.g., `/systems/abc-123`)

**Purpose:** Manage a single system - deploy, rollback, view history.

### Layout (Tabs)

The view has **tabs** for different aspects:

#### Tab 1: Overview (`/systems/:id`)

**Purpose:** See current system state at a glance.

**Shows:**
- System name and hostname
- Environment badge
- Status (online/offline, last heartbeat)
- NixOS version
- Currently deployed flake + commit
- Currently activated generation number

#### Tab 2: Deploy (`/systems/:id/deploy`)

**Purpose:** Deploy a new configuration to this system.

**Components:**
1. **Flake Selector** - Dropdown to pick a flake
2. **Branch Selector** - Pick branch (main, prod, etc.)
3. **Commit Selector** - Pick commit (shows commit message + date)
4. **Diff Viewer** - Shows what files changed (optional)
5. **Deploy Button** - Triggers deployment

**User Flow:**
1. User selects flake
2. User selects branch
3. User selects commit
4. (Optional) User clicks "Show Diff" to see changes
5. User clicks "Deploy"
6. Modal shows progress
7. Success/failure notification

#### Tab 3: History (`/systems/:id/history`)

**Purpose:** See past deployments to this system.

**Shows:**
- Table of deployments
- Columns: Date, Commit, Status (success/failed), Triggered By
- Click row to see deployment details

#### Tab 4: Logs (`/systems/:id/logs`)

**Purpose:** See deployment output logs.

**Shows:**
- Scrollable log output
- Timestamps
- Filter by deployment (select from dropdown)

### Data Flow

```
Frontend                         Backend
   │                               │
   ├─ GET /api/v1/systems/:id ──►│ Get system details
   │                               │
   ├─ GET /api/v1/systems/:id/deployments ──►│ Get history
   │                               │
   ├─ GET /api/v1/systems/:id/logs ──►│ Get logs
   │                               │
   ├─ POST /api/v1/systems/:id/deploy ──►│ Trigger deployment
```

### How to Modify

- **Backend:** `handlers/api/systems.rs`
- **Frontend:** `views/system_detail.rs`, `components/system/`

---

## Flakes List (`/flakes`)

**Route:** `/flakes`

**Purpose:** Manage the flake repositories Crystal Forge tracks.

### What It Shows

1. **Flake Cards**
   - Repository name
   - Git URL
   - Branch (e.g., "main", "nixos-unstable")
   - Last sync time
   - Sync status badge (synced ✅, syncing 🔄, error ❌)

2. **Filter Controls**
   - Environment filter
   - Sync status filter

3. **Actions**
   - **Sync Now** - Force git pull
   - **View Timeline** - See commits
   - **Add Flake** - Register new flake

### Adding a Flake (Modal)

Clicking "Add Flake" opens a modal with:
- **Name:** Display name (e.g., "Production Configs")
- **Repository URL:** Git HTTPS or SSH URL
- **Branch:** Default branch to track
- **Description:** Optional

### Flake Timeline (Sub-view)

Clicking a flake shows its commit history:

**Shows:**
- List of commits (newest first)
- Each commit shows:
  - Short SHA (e.g., `abc1234`)
  - Commit message
  - Author
  - Date
  - Number of changed files

**Interactions:**
- Click commit → Show changed files
- Click "Deploy" on commit → Opens deploy modal for that commit

### Data Flow

```
Frontend                  Backend
   │                        │
   ├─ GET /api/v1/flakes ─►│
   │                        │
   │◄─── { flakes: [...] }◄─│
   │                        │
   ├─ POST /api/v1/flakes ─►│ Add new flake
   │                        │
   ├─ POST /api/v1/flakes/:id/sync ──►│ Force sync
   │                        │
   ├─ GET /api/v1/flakes/:id/commits ──►│ Get timeline
```

### How to Modify

- **Backend:** `handlers/api/flakes.rs`, `queries/flakes.rs`
- **Frontend:** `views/flakes_list.rs`, `flake/adapter.rs`

---

## Environments List (`/environments`)

**Route:** `/environments`

**Purpose:** Group systems by environment (prod, staging, dev).

### What It Shows

1. **Environment Cards**
   - Environment name (e.g., "Production")
   - Color badge
   - System count
   - Assigned cache (future - TASK-141)

2. **Actions**
   - **Add Environment** - Create new environment
   - **Edit** - Change name/color

### What Is an Environment?

An environment is a **logical grouping** for systems:
- Production systems go in "prod" environment
- Staging systems go in "staging" environment
- Development systems go in "dev" environment

**Why?** Two reasons:
1. **Filtering:** See only prod systems in dashboard
2. **RBAC:** Users can be restricted to specific environments

### Data Flow

```
Frontend                      Backend
   │                            │
   ├─ GET /api/v1/environments ►│
   │                            │
   │◄─── { environments: [...] }◄│
```

### How to Modify

- **Backend:** `handlers/api/environments.rs`, `queries/environments.rs`
- **Frontend:** `views/environments_list.rs`

---

## Builds Queue (`/builds`)

**Route:** `/builds`

**Purpose:** Monitor the build queue and builder workers.

### What It Shows

#### Builder Workers Panel
- List of registered builders
- Each builder shows:
  - Name
  - Status (idle 🟢, building 🟡, paused 🔴)
  - Current job (if building)
  - CPU/RAM allocated

#### Build Queue Sections

1. **Pending** - Builds waiting to be picked up
2. **In Progress** - Currently building
3. **Recently Completed** - Last 10 builds with status

### Build States

A derivation goes through these states:

```
pending → building → built → cache-pushing → cache-pushed
                 ↓         ↓           ↓
              failed   cache-failed  cache-failed
```

### What Is a "Build"?

A build is a **Nix derivation** that needs to be built:
- Created when a new commit is detected
- Queued for an available builder
- Builder runs `nix build`
- On success: optionally push to cache
- Result reported back to server

### Data Flow

```
Frontend                    Backend
   │                         │
   ├─ GET /api/v1/builders ─►│ Get builder status
   │                         │
   ├─ GET /api/v1/build-queue ─►│ Get pending/in-progress
```

### How to Modify

- **Backend:** `handlers/api/builders.rs`, `builder/mod.rs`
- **Frontend:** `views/builds.rs`

---

## Admin Console (`/admin`)

**Route:** `/admin`

**Purpose:** Server administration - users, audit, OIDC mappings.

**Note:** Only accessible to users with **Admin** role.

### Tab 1: Users (`/admin/users`)

**Purpose:** Manage user accounts.

**Shows:**
- Table of users
- Columns: Email, Role, Status (enabled/disabled), Environments, Last Login

**Actions:**
- **Create User** - Add local user (email + password)
- **Edit User** - Change role, enable/disable
- **Assign Environments** - Add user to environments
- **Delete User** - Remove user

**User Types:**
1. **Local** - Created in CF with email/password
2. **IdP** - Created automatically from OIDC login

### Tab 2: Audit Log (`/admin/audit`)

**Purpose:** See who did what.

**Shows:**
- Table of audit events
- Columns: Timestamp, Actor (who), Action (what), Target (on what), IP Address

**Actions Logged:**
- User login/logout
- User create/update/delete
- Role changes
- Deployment triggered
- System registered
- Flake added/removed

**Filters:**
- Date range
- Actor (user)
- Action type

### Tab 3: OIDC Mappings (`/admin/oidc`)

**Purpose:** Map Identity Provider groups to CF roles.

**Shows:**
- List of mappings
- Columns: Group Name → Role, Group Name → Environments

**Example:**
| OIDC Group | CF Role | Environments |
|------------|---------|--------------|
| engineers | Operator | dev, staging |
| admins | Admin | all |
| execs | Viewer | prod |

### Data Flow

```
Frontend                     Backend
   │                          │
   ├─ GET /api/v1/admin/users ──►│
   │                          │
   ├─ POST /api/v1/admin/users ──►│ Create user
   │                          │
   ├─ GET /api/v1/admin/audit ──►│ Get audit log
   │                          │
   ├─ GET /api/v1/admin/oidc-mappings ──►│
```

### Authorization

All admin endpoints require `role = Admin`.

### How to Modify

- **Backend:** `handlers/api/admin.rs`
- **Frontend:** `views/admin.rs`

---

## Login Views

### Production Login (`/login`)

**Route:** `/login`

**Purpose:** Authenticate users via OIDC.

**Flow:**
1. User visits `/login`
2. Redirected to Identity Provider (Google, Okta, etc.)
3. User authenticates with IdP
4. Redirect back to CF with tokens
5. CF creates session, maps groups to roles
6. Redirect to Dashboard

### Dev Mode Login (`/dev/login`)

**Route:** `/dev/login`

**Purpose:** Local development without OIDC.

**Flow:**
1. User visits `/dev/login`
2. Sees three buttons: "Login as Admin", "Login as Operator", "Login as Viewer"
3. Clicks desired role
4. Dev user created (in-memory)
5. Redirect to Dashboard

**Warning Banner:** Shows "Development Mode Only - Do Not Use in Production"

---

## Common UI Components

### Loading States

- **Initial Load:** Skeleton loader (gray boxes)
- **Action in Progress:** Spinner + "Loading..." text
- **Optimistic Updates:** UI updates immediately, reverts on error

### Error States

- **API Error:** Red toast notification with message
- **Network Error:** "Unable to connect. Please check your connection."
- **Permission Error:** "You don't have permission to perform this action"

### Modals

Used for:
- Add/Edit forms
- Confirmations (delete, deploy)
- Viewing details

### Forms

- **Validation:** Real-time, inline errors
- **Submit:** Button disabled until valid
- **Success:** Modal closes, list refreshes

---

## Responsive Behavior

### Desktop (>1024px)
- Full sidebar with icons + labels
- All content visible

### Tablet (768-1024px)
- Icons-only sidebar (labels hidden)
- Hover to see labels

### Mobile (<768px)
- Hamburger menu in top bar
- Tap to open slide-out drawer
- Full navigation in drawer

---

## File Organization

```
web-ui/src/
├── main.rs                 # App entry, routing
├── AppShell.rsx           # Layout with sidebar
├── api/
│   ├── client.rs          # API fetch functions
│   └── models.rs         # TypeScript types
├── views/
│   ├── dashboard.rs
│   ├── systems_list.rs
│   ├── system_detail.rs
│   ├── flakes_list.rs
│   ├── environments_list.rs
│   ├── builds.rs
│   ├── admin.rs
│   └── login.rs
├── components/
│   ├── systems/
│   ├── flakes/
│   ├── builds/
│   └── admin/
└── adapters/              # Data fetching + state
    ├── systems_adapter.rs
    ├── flakes_adapter.rs
    └── ...
```

---

## Adding a New View

1. **Create component** in `views/`
2. **Add route** in `main.rs`
3. **Add nav item** in `AppShell.rsx`
4. **Add API functions** in `api/client.rs`
5. **Add adapter** in `adapters/` (if needed)

---

## Summary Table

| Route | View | Purpose |
|-------|------|---------|
| `/` | Dashboard | Fleet overview |
| `/systems` | Systems List | Browse systems |
| `/systems/:id` | System Detail | Manage one system |
| `/flakes` | Flakes List | Manage flakes |
| `/environments` | Environments List | Manage environments |
| `/builds` | Builds | Monitor queue |
| `/admin` | Admin | User/audit management |
| `/login` | Login | OIDC auth |
| `/dev/login` | Dev Login | Local dev auth |
