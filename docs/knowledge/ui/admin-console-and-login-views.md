---
type: UI Design
title: "Admin console and login views"
description: "Describes the Admin console tabs (users, audit log, OIDC mappings), their authorization, and the production OIDC and dev-mode login flows."
tags:
  - crystal-forge
  - ui
  - admin
  - login
  - oidc
  - rbac
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:29-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/specs/01-frontend-views.md at commit 3b23d36f"
    title: "Frontend Views Specification"
---

# Admin console and login views

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

> **Status:** The tab routes above (`/admin/users`, `/admin/audit`, `/admin/oidc`) are not separate routes in `packages/web-ui/src/routes.rs`, which registers a single `/admin` route. The Data Flow lists `GET /api/v1/admin/audit`; the server registers `/api/v1/admin/audit-events`. The login routes `/login` and `/dev/login` are registered. Not reconciled in this migration.

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

## Related concepts

- [Web UI navigation, shared components, responsive behavior, and structure](frontend-navigation-and-shared-patterns.md)
- [API authentication, sessions, and role-based authorization](../security/api-authentication-and-authorization.md)
- [Builders, queues, environments, dashboard, and admin APIs](../api/builders-queues-environments-dashboard-admin-api.md)
