---
id: TASK-414
title: Implement account notifications and active session management
status: In Progress
assignee:
  - gpt-5.5
created_date: '2026-08-01 04:04'
updated_date: '2026-08-02 02:11'
labels:
  - frontend
  - web-ui
  - backend
  - api
  - database
  - auth
  - security
  - notifications
  - email
  - design
  - testing
dependencies: []
references:
  - 'merge-request:312'
  - docs/design/CrystalForge/components/ProfileView.jsx
  - docs/design/CrystalForge/components/Shell.jsx
  - docs/design/CrystalForge/styles.css
  - packages/web-ui/src/views/profile.rs
  - packages/web-ui/src/components/layout/topbar.rs
  - packages/web-ui/src/components/layout/app_shell.rs
  - packages/web-ui/src/alerts/
  - packages/default/crates/cf-server/src/auth/
  - packages/default/crates/cf-server/src/handlers/api/
  - packages/default/crates/cf-server/migrations/0196_user_preferences.sql
  - 'merge-request:314'
  - 'https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/314'
modified_files:
  - packages/default/crates/cf-server/migrations/
  - packages/default/crates/cf-server/src/api/
  - packages/default/crates/cf-server/src/auth/
  - packages/default/crates/cf-server/src/handlers/api/
  - packages/default/crates/cf-server/src/models/
  - packages/default/crates/cf-server/src/queries/
  - packages/default/crates/cf-server/src/background_jobs/
  - packages/default/crates/cf-server/src/server/
  - packages/web-ui/src/api/
  - packages/web-ui/src/components/
  - packages/web-ui/src/components/layout/
  - packages/web-ui/src/state/
  - packages/web-ui/src/views/profile.rs
  - packages/web-ui/assets/
  - checks/web-ui/
  - modules/
priority: high
type: feature
ordinal: 410000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Complete the two intentionally unavailable areas added to the Profile & Preferences page by MR !312:

1. Notifications
2. Active sessions

Both features must use real account-scoped server data. They must not use mock sessions, static notification arrays, browser-only state, or controls that appear functional without changing application behavior.

The Profile page must visually match the reference design in:

- `docs/design/CrystalForge/components/ProfileView.jsx`
- `docs/design/CrystalForge/styles.css`

The design files are the visual and interaction reference only. Production behavior must use the Crystal Forge server, persistent database records, authenticated user identity, existing attention records, and existing authorization rules.

## Goals

- Persist notification preferences by authenticated `users.id`.
- Keep notification preferences synchronized across browsers and computers.
- Implement a functional notification center behind the top-bar bell.
- Deliver selected notification categories through in-app and email channels.
- Implement a durable, idempotent email-delivery queue.
- Implement the weekly digest preference.
- List the authenticated user’s active sessions.
- Identify the current session without exposing its token or hash.
- Allow the user to revoke another session.
- Allow the user to sign out everywhere.
- Prevent pending work or state from crossing user accounts.
- Match the Profile design for spacing, typography, controls, chips, buttons, responsive layout, and disabled or error states.
- Add complete server, database, web UI, and integration-test coverage.

## Out of scope

- SMS notifications.
- Mobile push notifications.
- Browser Web Push.
- Arbitrary webhook delivery.
- Administrator management of another user’s sessions.
- A user-configurable digest schedule.
- Password changes for OIDC users.
- Replacing the identity provider’s MFA or password-management flows.
- Suppressing creation of canonical attention, audit, compliance, build, deployment, CVE, or health records.
- Using email as the sole source of an operational or security record.
- Exposing session tokens, token hashes, CSRF tokens, OIDC tokens, or refresh tokens.

## Product semantics

### Notifications control delivery, not event production

Notification preferences control whether and where a user receives a notification.

They must not:

- prevent an attention occurrence from being created;
- remove an event from operational history;
- change build, deployment, evaluation, CVE, compliance, or health results;
- resolve an attention occurrence;
- change evidence or audit records;
- silently hide information from a destination view that the user opens directly.

Sidebar attention badges remain operational navigation state. Notification preferences control the top-bar notification feed and email delivery.

### Supported notification categories

Implement these user-facing categories from the design:

| Preference          | Canonical meaning                                                 |
| ------------------- | ----------------------------------------------------------------- |
| Deploy failures     | A deployment enters a failed terminal state                       |
| Build failures      | A build enters a failed terminal state                            |
| New critical CVEs   | A new critical CVE attention episode is opened                    |
| Policy violations   | A new policy, evaluation, or compliance failure episode is opened |
| Heartbeat lost      | A system crosses the configured lost or offline threshold         |
| Weekly digest email | A weekly summary of eligible events                               |

Do not create a new notification every time a producer reconciles the same condition.

A reopened condition after a resolved episode is a new notification. Repeated updates within the same episode are not new notifications unless the underlying canonical attention model creates a new occurrence.

### Delivery channels

Support:

- `in_app`
- `email`
- `both`

Definitions:

- `in_app`: include eligible events in the authenticated user’s notification center.
- `email`: deliver eligible events by email but do not include them in the notification center.
- `both`: use both delivery paths.

The server remains authoritative for the selected delivery channel.

### Production defaults

Use safe defaults for newly initialized users:

```text
Deploy failures: enabled
Build failures: enabled
New critical CVEs: enabled
Policy violations: enabled
Heartbeat lost: disabled
Weekly digest email: disabled
Delivery: in_app
```

Do not enable unsolicited email by default.

### Email availability

Email and weekly digest controls are available only when:

- server email delivery is configured and healthy;
- the authenticated user has an email address;
- the deployment classification and notification policy permit email delivery.

When email delivery is unavailable:

- keep `In-app` selectable;
- disable `Email` and `Both`;
- disable `Weekly digest email`;
- show a concise explanation in the card;
- do not silently accept a setting that cannot be delivered.

### Notification authorization

A user can receive only events that the same user is authorized to view in the destination API and route.

The notification query and email worker must enforce authorization on the server. The frontend must not be the authorization boundary.

A stored notification must not grant access to an underlying object after the user’s role or scope changes.

### Notification read and dismissal semantics

Each in-app notification has independent per-user state:

- unread;
- read;
- dismissed.

Reading or dismissing a notification does not resolve the underlying attention occurrence.

Clicking a notification must:

1. mark it read;
2. close the notification menu;
3. navigate to the relevant Crystal Forge route;
4. focus or identify the relevant subject when the destination supports deep linking.

### Historical behavior

Do not generate email for events that occurred before this feature was deployed or before the user’s notification state was initialized.

The initial notification center must not flood users with historical failures.

A limited recent backfill may be implemented only when it is deterministic and bounded. If implemented, use the existing attention episode timestamps and a maximum 24-hour lookback. Never send email for backfilled entries.

## Active-session semantics

### Session identity

Each authenticated session must have a stable, opaque session record ID that is separate from the session token.

The API must never return:

- the session token;
- the token hash;
- the CSRF token;
- the OIDC access token;
- the OIDC refresh token;
- provider credentials.

### Current session

The server must determine the current session from the authenticated session token.

The client must not submit or guess which session is current.

The current session row displays the `this device` chip and does not display a Revoke button.

The existing Sign out button remains the action for ending only the current session.

### Session metadata

Store or derive enough metadata to display:

- stable session ID;
- creation time;
- last activity time;
- expiration time;
- current-session status;
- authentication source;
- browser family;
- operating-system family;
- device class when it can be derived reliably;
- IP address, subject to the trusted-proxy rules below.

User-agent parsing is best effort. Unknown values must render as `Unknown browser`, `Unknown device`, or another explicit unavailable state. Do not invent device names.

### Session ordering

Return active sessions in this order:

1. current session;
2. remaining sessions by `last_seen_at DESC`;
3. `created_at DESC` as the stable tie-breaker.

Expired and revoked sessions are not active and must not appear in the default list.

### Individual revocation

A user can revoke one of their own non-current sessions.

The operation must:

- derive `user_id` from the authenticated request;
- verify that the session belongs to that user;
- revoke the session atomically;
- be idempotent;
- remove the session row from the active list after success;
- cause the revoked session cookie to be rejected on its next request.

Attempting to revoke another user’s session must return a not-found response or another non-disclosing authorization response.

### Sign out everywhere

`Sign out everywhere` revokes every active session for the authenticated user, including the current session.

After success:

- clear the current session cookie;
- clear the CSRF cookie;
- clear frontend authentication and user-specific state;
- redirect to `/login`;
- prevent back-navigation from restoring authenticated UI.

Show a confirmation dialog before performing this operation.

The confirmation must clearly state that all computers and browsers will be signed out.

## Database changes

Use the next available migration number after rebasing on current `dev`. Do not hard-code a migration number before the branch is rebased.

### Notification preferences

Add a dedicated account-scoped table or an equivalent normalized representation:

```sql
CREATE TABLE user_notification_preferences (
    user_id UUID PRIMARY KEY
        REFERENCES users(id)
        ON DELETE CASCADE,

    deploy_failures BOOLEAN NOT NULL DEFAULT TRUE,
    build_failures BOOLEAN NOT NULL DEFAULT TRUE,
    critical_cves BOOLEAN NOT NULL DEFAULT TRUE,
    policy_violations BOOLEAN NOT NULL DEFAULT TRUE,
    heartbeat_lost BOOLEAN NOT NULL DEFAULT FALSE,
    weekly_digest BOOLEAN NOT NULL DEFAULT FALSE,

    delivery_channel TEXT NOT NULL DEFAULT 'in_app'
        CHECK (delivery_channel IN ('in_app', 'email', 'both')),

    initialized_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

The exact schema can differ, but it must preserve the listed behavior and constraints.

### User notifications

Add durable per-user in-app notification state:

```sql
CREATE TABLE user_notifications (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL
        REFERENCES users(id)
        ON DELETE CASCADE,

    category TEXT NOT NULL,
    source_occurrence_id UUID,
    source_type TEXT NOT NULL,
    source_id TEXT NOT NULL,

    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    route TEXT NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at TIMESTAMPTZ,
    dismissed_at TIMESTAMPTZ,

    UNIQUE (user_id, category, source_occurrence_id)
);
```

If an existing attention occurrence uses a different identifier type, use that type consistently.

Required indexes:

- unread notifications by user and creation time;
- visible notifications by user and creation time;
- source occurrence lookup;
- cleanup and retention queries.

### Email deliveries

Add a durable email-delivery queue with:

- delivery ID;
- user ID;
- notification ID when applicable;
- delivery type: immediate or weekly digest;
- idempotency key;
- state: pending, sending, sent, failed, cancelled;
- attempt count;
- next attempt time;
- claim time;
- sent time;
- last error;
- creation and update timestamps.

The idempotency key must prevent duplicate delivery when:

- a producer retries;
- the server restarts;
- a worker claim becomes stale;
- the same attention occurrence is reconciled more than once.

### Weekly digest runs

Record digest periods so each user receives at most one digest for a given period.

At minimum persist:

- user ID;
- period start;
- period end;
- status;
- delivery ID;
- creation time;
- sent time;
- error details.

Use a uniqueness constraint on the user and digest period.

Do not send an empty digest.

### Session records

Extend the existing session persistence model as needed with:

- stable session record ID;
- user ID;
- token hash;
- created time;
- last-seen time;
- expiration time;
- revoked time;
- IP address;
- user-agent string;
- authentication source.

Use PostgreSQL `INET` for IP addresses when practical.

Existing sessions must remain valid after migration. Missing metadata for an existing session must render as unavailable, not as fabricated data.

### Last-seen write throttling

Do not update `last_seen_at` on every request.

Update it at most once per configured interval, with a default interval of five minutes. The session remains valid between updates.

This prevents session tracking from adding a database write to every API request.

### Retention

Retain revoked and expired session records long enough for audit and troubleshooting, then remove them through a bounded cleanup job.

Make the retention period configurable. Use a safe default such as 30 days.

Do not retain raw session tokens.

## API requirements

All routes require authentication and CSRF protection where applicable.

The request body must never contain an authoritative `user_id`.

Always derive the user from the authenticated session.

### Notification preferences

```text
GET   /api/v1/user/notification-preferences
PATCH /api/v1/user/notification-preferences
```

The GET response includes:

- current preference values;
- whether email delivery is available;
- the authenticated delivery email, when available;
- a concise unavailability reason when email cannot be used.

PATCH must support partial updates and update only supplied fields.

Validate delivery-channel values with typed enums.

### Notification center

```text
GET  /api/v1/user/notifications
POST /api/v1/user/notifications/:notification_id/read
POST /api/v1/user/notifications/read-all
DELETE /api/v1/user/notifications/:notification_id
```

The list endpoint supports:

- cursor pagination;
- a bounded page size;
- unread-only filtering;
- newest-first ordering;
- total unread count or a separate unread-count field.

Do not implement an unbounded notification list.

Read, read-all, and dismiss operations can modify only records owned by the authenticated user.

### Sessions

```text
GET    /api/v1/user/sessions
DELETE /api/v1/user/sessions/:session_id
POST   /api/v1/user/sessions/revoke-all
```

`GET /sessions` returns only the current user’s active sessions.

Each session response includes:

```json
{
  "id": "opaque-session-uuid",
  "current": true,
  "device_label": "Linux · Chrome",
  "browser": "Chrome",
  "operating_system": "Linux",
  "device_class": "desktop",
  "ip_address": "192.0.2.10",
  "auth_source": "oidc",
  "created_at": "2026-08-01T04:00:00Z",
  "last_seen_at": "2026-08-01T04:10:00Z",
  "expires_at": "2026-08-08T04:00:00Z"
}
```

Do not return sensitive token material.

`DELETE /sessions/:id` must reject attempts to revoke the current session through the individual-revoke route. The user must use Sign out or Sign out everywhere for the current session.

`POST /sessions/revoke-all` revokes all sessions and clears the current authentication cookies in the same response.

## Notification event integration

Use the existing canonical attention system as the source when it already represents the required event.

Do not create a competing set of event-detection rules in the notification worker.

Map canonical attention categories to notification categories in one server-side module.

At minimum integrate:

- deployment failures;
- build failures;
- critical CVE episodes;
- policy or evaluation failure episodes;
- lost-heartbeat or offline-system episodes.

### Deduplication

Use the canonical occurrence or episode ID as the primary deduplication identity.

A producer retry must not create another user notification or another email delivery.

A resolved condition that later reopens as a new attention episode can generate a new notification.

### Recipient evaluation

Evaluate recipients using current user authorization and preference data.

Do not create a delivery for a user who cannot view the underlying subject.

Recheck current authorization and current preferences before sending email from the queue.

A user who disables email while a delivery is pending must not receive that pending delivery unless it has already been handed to the email provider.

## Email delivery

### Configuration

Add NixOS module and server configuration for:

- email enabled;
- SMTP or supported provider endpoint;
- sender address;
- sender display name;
- TLS mode;
- credentials through existing secret-management patterns;
- immediate-delivery worker interval;
- retry limits;
- digest schedule;
- content policy.

Never write email credentials to logs, API responses, generated JavaScript, or the database.

### Worker behavior

Use a durable PostgreSQL queue with:

- atomic claims;
- `FOR UPDATE SKIP LOCKED` or the project’s standard queue pattern;
- bounded batches;
- stale-claim recovery;
- exponential retry backoff;
- maximum attempts;
- structured error recording;
- graceful shutdown;
- no duplicate sends after restart.

Mark a delivery as sent only after the provider confirms acceptance.

### Email content

Immediate email contains:

- notification category;
- concise title;
- event time;
- severity;
- safe human-readable summary;
- link to the relevant Crystal Forge route.

Weekly digest contains:

- covered time period;
- counts by category;
- a bounded list of recent items;
- links to relevant Crystal Forge views.

Do not include:

- credentials;
- session identifiers;
- session IP addresses;
- full build logs;
- Nix expressions;
- policy source;
- access tokens;
- secret values;
- unbounded error output.

HTML-escape and text-escape all user-controlled values.

Provide both text and HTML MIME parts.

### Classification restrictions

Email delivery must respect the deployment’s classification and external-delivery policy.

When external email is prohibited:

- report email as unavailable through the preferences API;
- disable email controls;
- keep in-app notifications functional;
- do not create email deliveries.

## Trusted proxy and IP handling

Do not blindly trust `X-Forwarded-For`, `Forwarded`, or similar headers.

Use forwarded client addresses only when:

- trusted proxy handling is explicitly configured;
- the direct peer address is within the configured trusted-proxy set;
- the header is parsed with a bounded and documented algorithm.

Otherwise use the direct peer address.

The UI must not label a proxy address as the client address when the server cannot determine the client address safely.

## Web UI requirements

### Shared state

Notification preferences and notification-center state must be account-scoped and owned at the authenticated `AppShell` lifecycle or an equivalent user-owned context.

Do not use:

- component-local persistence as the authoritative source;
- thread-local queues that survive logout;
- browser-only preferences;
- state that can cross authenticated users.

Authentication changes must:

- clear pending saves;
- clear notification feed state;
- clear session-list state;
- invalidate in-flight responses from the previous user;
- initialize clean state for the next user.

### Notifications Profile card

Replace the unavailable card with the design controls:

```text
Notifications
  Deploy failures              [toggle]
  Build failures               [toggle]
  New critical CVEs            [toggle]
  Policy violations            [toggle]
  Heartbeat lost               [toggle]
  Weekly digest email          [toggle]
  Delivery                     [In-app | Email | Both]
```

Match the reference design:

- card padding;
- 13-pixel section title treatment;
- `PrefRow` spacing and dividers;
- checkbox or toggle sizing;
- segmented-control sizing;
- text hierarchy;
- muted descriptions;
- two-column placement beside Appearance on wide screens;
- single-column responsive layout on narrow screens.

Use shared CSS classes instead of adding new inline styling for every row.

### Notification preference behavior

- Load values from the server.
- Update controls optimistically.
- Serialize and coalesce rapid saves.
- Prevent stale responses from overwriting newer actions.
- Keep the final server value equal to the user’s last action.
- Show a visible error when a save fails.
- Restore or clearly mark unsaved state after failure.
- Clear an old error after a later save succeeds.
- Do not block the whole application while preferences load.
- Preserve local cached values only as a startup cache.
- Server values are authoritative.

### Top-bar notification center

Make the existing bell functional.

The bell must display:

- an unread indicator or bounded unread count;
- an accessible label that includes the unread count;
- an interactive menu, popover, or narrow-screen drawer.

The notification center must contain:

- newest-first notification rows;
- category or severity icon;
- title;
- concise summary;
- relative time;
- absolute timestamp in a tooltip or secondary presentation;
- unread visual treatment;
- mark-read behavior;
- dismiss behavior;
- mark-all-read action;
- empty state;
- loading state;
- explicit retryable error state;
- pagination or incremental loading.

Clicking outside and pressing Escape must close the menu.

Keyboard focus must return to the bell after the menu closes.

Support Arrow Up, Arrow Down, Home, and End navigation when using menu semantics.

Do not display a static mock list.

### Active Sessions Profile card

Replace the unavailable card with the design layout:

- heading: `Active sessions`;
- vertically stacked session rows;
- server or device icon;
- device and browser label;
- monospaced IP and activity line;
- `this device` chip for the current session;
- `Revoke` button for other sessions;
- warning-styled `Sign out everywhere` button below the list.

Match the reference dimensions and visual treatment:

- 18-pixel card padding;
- 8-pixel row gaps;
- subtle row background;
- 8-pixel row radius;
- 12-pixel main row text;
- 10-pixel metadata text;
- healthy chip for the current device;
- warning color and border for Sign out everywhere.

### Session states

Provide:

- loading skeleton or bounded loading state;
- empty state that still explains the current session if data is unavailable;
- explicit API error and Retry action;
- per-row revoke progress;
- per-row revoke error;
- revoke-all progress;
- revoke-all confirmation dialog;
- revoke-all error;
- automatic list refresh after successful revocation.

Do not remove a session optimistically unless it can be restored correctly after failure.

### Accessibility

- Every toggle has a visible label and accessible name.
- Segmented controls expose selected state.
- Session Revoke buttons identify the target device in their accessible labels.
- The confirmation dialog traps focus.
- Escape closes the dialog.
- Focus returns to Sign out everywhere after cancellation.
- Status and error messages use appropriate live regions.
- Color is not the only unread, current-session, error, or disabled indicator.
- Touch targets meet the project’s existing minimum dimensions.

## Audit requirements

Create audit events for:

- notification preference changes;
- individual session revocation;
- sign out everywhere;
- email delivery permanently failing after retries;
- administrative changes to email-delivery configuration when such audit support already exists.

Audit records must identify:

- authenticated actor;
- affected session or preference category;
- action time;
- result;
- request correlation identifier.

Do not store session tokens or email credentials in audit details.

## Concurrency and lifecycle requirements

### Preference updates

Rapid updates must preserve last-action-wins behavior.

For example:

1. User selects Email.
2. User immediately selects Both.
3. Requests complete out of order.
4. The final database value must be Both.

### Session revocation

A revoke operation racing with an authenticated request must have deterministic behavior.

After the revocation transaction commits, later requests using that session must fail authentication.

### Account switching

This sequence must not leak state:

1. User A opens Notifications or Active Sessions.
2. User A starts a request.
3. User A signs out.
4. User B signs in in the same browser tab.
5. User A’s response arrives.

The stale response must be discarded and must not modify User B’s state.

### Email queue

Concurrent workers must not send the same delivery twice.

Stale claims must recover without losing the delivery or violating idempotency.

## Testing requirements

### Database tests

Test:

- default notification preference creation;
- preference persistence across sessions;
- partial preference updates;
- delivery-channel validation;
- user isolation;
- notification uniqueness by source occurrence;
- read and dismiss ownership;
- email-delivery idempotency;
- weekly digest uniqueness;
- session ordering;
- current-session identification;
- individual revocation;
- revoke-all;
- expired-session exclusion;
- revoked-session exclusion;
- last-seen throttling;
- existing-session migration with missing metadata.

### Server API tests

Test:

- authenticated preference GET and PATCH;
- missing authentication;
- CSRF enforcement;
- inability to supply another user ID;
- email capability reporting;
- email unavailable behavior;
- paginated notification listing;
- unread filtering;
- mark read;
- mark all read;
- dismiss;
- cross-user notification access denial;
- session listing;
- individual session revocation;
- current-session revoke rejection;
- cross-user session revoke denial;
- revoke-all cookie clearing;
- revoked-token rejection;
- trusted-proxy IP extraction;
- untrusted forwarded-header rejection.

### Notification producer tests

For every supported category, verify:

- a new canonical episode creates one eligible notification;
- producer retries do not create duplicates;
- repeated reconciliation does not create duplicates;
- a new episode after resolution creates a new notification;
- disabled categories do not create a delivery for that user;
- authorization filters recipients;
- email-only excludes the in-app feed;
- in-app-only excludes the email queue;
- both creates both delivery paths.

### Email worker tests

Test:

- successful immediate delivery;
- provider failure retry;
- exponential backoff;
- maximum-attempt terminal failure;
- stale-claim recovery;
- concurrent claims;
- current-preference recheck;
- current-authorization recheck;
- no empty weekly digest;
- one digest per period;
- text and HTML escaping;
- no sensitive fields in rendered messages.

Use an in-process fake provider. Tests must not require public network access.

### Web UI unit tests

Test:

- notification preference state mapping;
- save serialization;
- last-action-wins behavior;
- stale-response rejection;
- auth-generation reset;
- email capability states;
- notification unread-count rendering;
- session current-device rendering;
- session ordering;
- revoke-all state transitions.

### Web UI integration tests

Add Profile coverage that verifies:

1. Notification controls match the design labels.
2. Preferences load from the API.
3. A changed preference survives reload.
4. The same user sees the preference in a second browser context.
5. A different user receives independent defaults.
6. Email controls disable when email delivery is unavailable.
7. Save failures produce a visible error.
8. A later successful save clears the error.
9. The bell displays the unread count.
10. Opening the bell shows real API-shaped notifications.
11. Clicking a notification marks it read and navigates.
12. Mark all read clears the unread count.
13. Notification API errors do not appear as permanent loading.
14. Active sessions render the current session first.
15. Only non-current sessions have Revoke buttons.
16. Revoking a session removes it after server confirmation.
17. Revoking a session from User A does not affect User B.
18. Sign out everywhere requires confirmation.
19. Sign out everywhere redirects to login.
20. The revoked current cookie cannot reopen the application.
21. Narrow viewport layout matches the responsive design.
22. Keyboard navigation and focus restoration work.

### Nix and CI verification

Required commands include:

```bash
nix develop -c bash -c '
  SQLX_OFFLINE=true cargo check \
    --manifest-path packages/default/crates/cf-server/Cargo.toml
'

nix develop -c bash -c '
  cd packages/web-ui &&
  cargo check --target wasm32-unknown-unknown
'

nix develop -c bash -c '
  SQLX_OFFLINE=true cargo test \
    --manifest-path packages/default/crates/cf-server/Cargo.toml \
    user_notifications \
    --lib
'

nix develop -c bash -c '
  SQLX_OFFLINE=true cargo test \
    --manifest-path packages/default/crates/cf-server/Cargo.toml \
    user_sessions \
    --lib
'

nix develop -c bash -c '
  cd packages/web-ui &&
  cargo test --bin crystal-forge-ui
'

node --check checks/web-ui/tests/integration-test.js

nix build .#checks.x86_64-linux.web-ui --no-link
```

Run database-backed ignored tests explicitly against the migrated isolated test database.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

- [ ] The Notifications card is no longer disabled.
- [ ] The Notifications card matches the reference design.
- [ ] Notification preferences are stored by authenticated `users.id`.
- [ ] Notification preferences persist across browsers and computers.
- [ ] Notification preferences remain isolated between users.
- [ ] Rapid preference changes preserve the user’s final action.
- [ ] Preference API failures are visible and retryable.
- [ ] Email and Both disable when email delivery is unavailable.
- [ ] Email delivery is opt-in.
- [ ] Weekly digest email is opt-in.
- [ ] The top-bar bell opens a functional notification center.
- [ ] The notification center uses real server data.
- [ ] The unread count is account-specific and durable.
- [ ] Users can mark one notification read.
- [ ] Users can mark all notifications read.
- [ ] Users can dismiss a notification.
- [ ] Reading or dismissing a notification does not resolve operational attention.
- [ ] Notification creation is deduplicated by canonical occurrence or episode.
- [ ] Existing historical events do not trigger email after migration.
- [ ] Notification delivery respects current authorization.
- [ ] Notification delivery respects current user preferences.
- [ ] Immediate email uses a durable idempotent queue.
- [ ] Weekly digests are generated at most once per user per period.
- [ ] Empty weekly digests are not sent.
- [ ] Email content is escaped and contains no sensitive data.
- [ ] Classification policy can disable external email.
- [ ] The Active Sessions card is no longer disabled.
- [ ] The Active Sessions card matches the reference design.
- [ ] Session rows use real session records.
- [ ] The current session displays `this device`.
- [ ] The current session does not display a Revoke button.
- [ ] Other sessions can be revoked.
- [ ] A revoked session is rejected on its next request.
- [ ] Users cannot list or revoke another user’s sessions.
- [ ] Sign out everywhere revokes all active sessions.
- [ ] Sign out everywhere clears cookies and frontend auth state.
- [ ] Sign out everywhere redirects to `/login`.
- [ ] Session tokens and hashes never appear in API responses or logs.
- [ ] Session IP handling uses trusted-proxy configuration.
- [ ] Session last-seen updates are throttled.
- [ ] Stale responses are discarded after logout or account switching.
- [ ] Loading, empty, error, and retry states are implemented.
- [ ] Desktop and narrow-screen layouts match the design system.
- [ ] Keyboard navigation and focus restoration pass integration tests.
- [ ] Server tests pass.
- [ ] Database tests pass against migrated PostgreSQL.
- [ ] WASM cargo check passes.
- [ ] Web UI unit tests pass.
- [ ] Web UI integration tests pass.
- [ ] The Nix web-ui check passes.
- [ ] The task record documents the final schema, API routes, configuration, defaults, and verification commands.

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
## Implementation Plan

### Preflight and branch
- Worktree: `/home/mcamp/code/crystal-forge/TASK-414-account-notifications-sessions`
- Branch: `TASK-414-account-notifications-sessions`
- Base: local `dev` at `019f6fff` (Backlog auto-commit `Update task TASK-414`; parent `182823d0 add new task for notifications`)
- Next migration after current branch: `0197_*` unless `dev` is rebased and a later migration exists.

### Server/session foundation
1. Extend existing `user_sessions` rather than introducing a parallel session store. Preserve existing `id`, `session_token_hash`, `issued_at`, `expires_at`, `last_seen_at`, `invalidated_at`, `user_agent`, and `ip_address` semantics.
2. Add query/API support for listing the authenticated user’s active sessions, identifying the current row by the authenticated session token hash, revoking another owned session idempotently, and revoking all owned sessions while clearing session/CSRF cookies.
3. Add throttled `last_seen_at` updates in session resolution with a default five-minute interval, and add typed config/NixOS options for session last-seen throttle, retention, and trusted proxy client-IP handling.
4. Parse user-agent metadata best-effort server-side for browser/OS/device labels; render explicit unknown values rather than invented device names.
5. Add audit events for individual revocation and sign-out-everywhere without storing token material.

### Server/notification foundation
1. Add durable tables for `user_notification_preferences`, `user_notifications`, email delivery queue, and weekly digest runs, keyed by authenticated `users.id` and source occurrence/event ids with uniqueness for idempotency.
2. Implement typed server DTOs and authenticated routes:
   - `GET/PATCH /api/v1/user/notification-preferences`
   - `GET /api/v1/user/notifications`
   - `POST /api/v1/user/notifications/:notification_id/read`
   - `POST /api/v1/user/notifications/read-all`
   - `DELETE /api/v1/user/notifications/:notification_id`
   - `GET/DELETE/POST /api/v1/user/sessions...`
3. Map canonical attention occurrences to notification categories for build failures, CVEs, eval/policy-ish failures, and heartbeat/system offline events; use `system_events` for deployment failures because deployment failures are not currently attention occurrences.
4. Recheck authorization and current preferences when materializing notifications and before email send. Notification records must not grant access after role/scope changes.
5. Introduce an email delivery abstraction with an in-process fake provider for tests, durable queue claims/retries/stale-claim recovery, and weekly digest uniqueness/no-empty-digest behavior. Add config/NixOS options for email enablement, sender, transport, retry/worker intervals, digest schedule, and classification/external-delivery policy.

### Web UI
1. Add web DTOs/API client methods matching server routes, using existing CSRF helpers for mutating routes.
2. Add authenticated AppShell-owned notification/preferences/session contexts with auth-generation guards that clear pending saves, feed, and sessions on logout or account switch.
3. Replace Profile placeholders with real Notification controls and Active Sessions rows matching `docs/design/CrystalForge/components/ProfileView.jsx` and shared CSS patterns.
4. Replace TopBar static notification arrays with server-backed feed/unread count, read/dismiss/mark-all behavior, pagination or incremental loading, keyboard navigation, Escape/click-outside close, and focus restoration.

### Verification
- Run targeted server checks/tests with `SQLX_OFFLINE=true` during implementation.
- Run web WASM check and web unit tests.
- Run `node --check checks/web-ui/tests/integration-test.js` after integration-test edits.
- Run DB-backed ignored tests against an isolated migrated database when available.
- Run `nix build .#checks.x86_64-linux.web-ui --no-link` before review or report exact environment limitation if it cannot complete.
<!-- SECTION:PLAN:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Repaired malformed TASK-414 front matter so Backlog.md can hydrate the task correctly; issue was indentation under labels/references/modified_files plus a leading blank line before front matter. User explicitly requested starting TASK-414, so task is being taken into progress for implementation preflight.

Implementation started in worktree `/home/mcamp/code/crystal-forge/TASK-414-account-notifications-sessions` on branch `TASK-414-account-notifications-sessions`.

Implemented first backend/UI foundation slice:
- Added migration `0197_user_notifications_sessions.sql` with notification preference, in-app notification, email delivery queue, weekly digest run tables, plus `user_sessions.auth_source` and active-session index.
- Added server notification preference/list/read/read-all/dismiss API routes and DTO/query/model scaffolding.
- Added server user session list/revoke/revoke-all API routes, current-session derivation from the cookie token hash, CSRF enforcement for mutations, and throttled `last_seen_at` touch on authenticated requests.
- Added web DTOs/API client methods for notification preferences, notifications, and sessions.
- Replaced Profile notification/session placeholders with server-backed controls/session rows and sign-out-everywhere confirmation.

Verification run so far:
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo fmt && cargo check --target wasm32-unknown-unknown'` passed with existing warnings, but running workspace fmt touched unrelated server files; those unrelated formatting-only changes were reverted.

Continued TASK-414 implementation:
- Added server configuration fields for notification email enablement, endpoint, sender, TLS mode, credential file path, worker interval, max attempts, external-delivery policy gate, session last-seen throttle, and session retention.
- Added matching NixOS module options under `services.crystal-forge.server.notificationEmail`, plus session throttle/retention options, and wired them into generated TOML without copying credentials.
- Updated notification preference API to report real email capability from server config + user email and to allow `email`/`both`/weekly digest only when configured and policy-allowed.
- Extended attention notification materialization to enqueue durable idempotent immediate email delivery rows for `email`/`both` preferences.
- Added weekly digest enqueue query with no-empty-digest and one-run-per-period uniqueness semantics.
- Fixed Android user-agent parsing so Android is not mislabeled as Linux.
- Added focused unit tests for notification email capability DTO behavior and session user-agent parsing.

Verification in this slice:
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` passed: 2 tests.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_sessions --lib'` passed: 3 tests.

Note: an earlier parallel test run timed out while waiting on Cargo/Nix locks; the same targeted test commands were rerun serially and passed.

Continued TASK-414 implementation:
- Added `tasks::user_notification_email`, a configured background loop for durable email queue processing.
- Implemented atomic delivery claims with `FOR UPDATE SKIP LOCKED`, stale-claim recovery, preference/email rechecks before send, cancellation for unavailable recipients or disabled preferences, sent-state updates, retry/backoff helper, and bounded digest rendering.
- Added text/HTML rendering helpers with escaping and tests for controlled-value escaping.
- Wired the notification email loop into server background task startup; it exits disabled unless email transport config and external-delivery policy are both enabled.
- Updated web integration test script to stop expecting the old notification/session placeholders and to assert server-shaped notification/session UI states with mocked API responses for screenshot coverage.

Verification in this slice:
- `node --check checks/web-ui/tests/integration-test.js` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` passed: 4 tests.

Diagnosed the prior `nix build .#checks.x86_64-linux.web-ui --no-link` failure: Nix could not see the newly added TASK-414 Rust modules because they were still untracked in the Git worktree. Staged the intended new files so the flake source includes them. Verification run after staging:
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` — passed with existing warnings.
- `nix build .#checks.x86_64-linux.web-ui --no-link` — passed.
- `node --check checks/web-ui/tests/integration-test.js` — passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` — passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui'` — passed: 131 passed, 0 failed, 1 ignored.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` — passed: 4 passed, 0 failed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_sessions --lib'` — passed: 3 passed, 0 failed.
Remaining limitations: DB-backed ignored tests were not run because `CRYSTAL_FORGE_TEST_DATABASE_URL` is not set; SQLx metadata has not been refreshed in this session. Implementation remains uncommitted/unpushed pending authorization.

Committed and pushed TASK-414 implementation commit `7efeaedb` (`Implement account notifications and sessions`) to branch `TASK-414-account-notifications-sessions`. Opened MR !314 for review: https://gitlab.com/crystal-forge/crystal-forge/-/merge_requests/314. Task remains In Progress rather than Review because required DB-backed ignored tests have not been run locally (`CRYSTAL_FORGE_TEST_DATABASE_URL` unavailable) and SQLx metadata was not refreshed in this session; MR is open for early human/dev-server review and iteration.

Review result received for MR !314: request changes with seven P1 blockers and one P2. Blocking areas: invalid immediate-email materialization SQL/idempotency key, worker marking email sent without transport acceptance, missing notification authorization boundaries, missing CSRF validation on notification mutations, email/digest production depending on notification-center GET, preference write/client response races, former-account responses populating new-account UI. P2: session last-seen config not wired from ServerConfig and retention cleanup missing. Proceeding to address these in the TASK-414 branch/MR.

Continued MR !314 review-fix work:
- Added fake email transport regression coverage for durable email delivery acceptance, retry, terminal failure, and weekly digest period-boundary behavior (DB-backed tests are `#[ignore]` and require a migrated `CRYSTAL_FORGE_TEST_DATABASE_URL`).
- Changed weekly digest rendering to use the persisted `user_notification_weekly_digest_runs` period instead of all current notifications.
- Updated delivery state transitions to keep weekly digest run status in sync when sent, skipped/cancelled, retried, or terminally failed.
- Added web UI unit coverage for notification preference merge/coalescing last-action-wins behavior.

Verification run after this slice:
- `nix develop -c rustfmt --edition 2024 packages/default/crates/cf-server/src/tasks/user_notification_email.rs packages/web-ui/src/views/profile.rs` passed.
- `git diff --check` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` passed: 4 passed, 4 ignored.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_preference_merge'` passed: 2 passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `node --check checks/web-ui/tests/integration-test.js` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_sessions --lib'` passed: 3 passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui'` passed: 133 passed, 1 ignored.
- `nix build .#checks.x86_64-linux.web-ui --no-link` was attempted twice; it did not complete within 120s, then within 900s. The tool killed it and reported `error: interrupted by the user`, so final pass/fail is unverified.

Remaining limitations: DB-backed ignored tests have still not been executed because no `CRYSTAL_FORGE_TEST_DATABASE_URL` is available in this session; SQLx offline metadata has still not been refreshed. Review-fix changes remain uncommitted/unpushed.

Committed and pushed review-fix commit `57c7b1b7` (`Address notification review blockers`) to branch `TASK-414-account-notifications-sessions` for MR !314. Worktree status after push is clean. Remaining unverified items are unchanged: DB-backed ignored tests require `CRYSTAL_FORGE_TEST_DATABASE_URL`; SQLx metadata has not been refreshed; latest `nix build .#checks.x86_64-linux.web-ui --no-link` did not complete within the tool timeout and is unverified.

Continued MR !314 re-review fixes:
- Reworked notification email config to match the implemented HTTP provider transport: removed unsupported SMTP/TLS/credential-file options, added bounded request timeout and fixed weekly UTC digest schedule config in server config and NixOS module.
- Tightened notification idempotency and recurrence: attention notifications now identify immediate email deliveries by canonical `attention_occurrence` id, deployment/system-event deliveries by `system_event` id, and the migration now allows reopened attention occurrences while keeping deployment source uniqueness.
- Made weekly digest enqueue deterministic per UTC week (`date_trunc('week', NOW()) - 7 days` to `date_trunc('week', NOW())`) and pass the configured digest schedule into the worker.
- Added email transport idempotency key propagation through the HTTP `Idempotency-Key` header, bounded reqwest timeout, current preference rechecks for immediate/digest rendering, persisted digest-period rendering, severity in immediate content, and digest period/category-count content.
- Added frontend auth-generation/account guards for notification loads and topbar notification mutations, shared auth context set/clear helpers, profile logout/revoke-all state clearing, and synchronous preference save state to avoid stale optimistic response overwrites.

Verification for this slice:
- `nix develop -c rustfmt --edition 2024 packages/default/crates/cf-config/src/config/server.rs packages/default/crates/cf-server/src/handlers/api/user_notifications.rs packages/default/crates/cf-server/src/queries/user_notifications.rs packages/default/crates/cf-server/src/tasks/user_notification_email.rs packages/web-ui/src/state/app_state.rs packages/web-ui/src/bootstrap/auth.rs packages/web-ui/src/components/layout/app_shell.rs packages/web-ui/src/components/layout/topbar.rs packages/web-ui/src/views/login.rs packages/web-ui/src/views/profile.rs && git diff --check && node --check checks/web-ui/tests/integration-test.js` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` passed: 4 passed, 4 ignored.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_sessions --lib'` passed: 3 passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_preference_merge'` passed: 2 passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui'` passed: 133 passed, 1 ignored.
- `nix build .#checks.x86_64-linux.web-ui --no-link` was attempted again with a 900s tool timeout; it did not complete before the tool killed it and reported `error: interrupted by the user`, so final pass/fail remains unverified.

Remaining limitations: DB-backed ignored tests still require a migrated `CRYSTAL_FORGE_TEST_DATABASE_URL` and were not run in this session; SQLx offline metadata has not been refreshed.

Committed and pushed latest re-review-fix commit `08639ffb` (`Tighten notification delivery review fixes`) to branch `TASK-414-account-notifications-sessions` for MR !314. Worktree status after push is clean. Remaining limitations: DB-backed ignored tests require `CRYSTAL_FORGE_TEST_DATABASE_URL`; SQLx metadata has not been refreshed; `nix build .#checks.x86_64-linux.web-ui --no-link` was attempted but timed out after 900s and remains unverified.

Continued MR !314 re-review fixes:
- Added persisted email opt-in cutoffs to `user_notification_preferences` for each immediate category plus weekly digest. Preference updates now set/reset those timestamps when email-capable delivery or a category/digest is enabled/disabled.
- Immediate email enqueue now requires the source event timestamp to be at or after the relevant email cutoff, preventing replay of older in-app-only events when a user later switches to `email`/`both` or re-enables a category.
- Weekly digest enqueue now requires `weekly_digest_enabled_at` before the completed period start and only counts/items after the digest cutoff, preventing a newly enabled digest from sending the previous week.
- Removed the `resolved_at IS NULL` filter from attention materialization so resolved-but-unseen occurrences after the durable preference cutoff can still create exactly one notification/delivery by occurrence id.
- Added `public_base_url`, provider token file, and explicit loopback-only insecure HTTP development option to email configuration and NixOS options. Validation now rejects unsafe/missing public origins, non-loopback plaintext endpoints, and provider token files in `/nix/store`.
- Email transport now sends bearer auth from the configured runtime secret file and expands stored application routes against `public_base_url` for text and HTML email links.
- Weekly digest rendering now uses an unrestricted grouped category-count query plus a separately bounded recent-item query.
- Topbar notification clicks now only update local read/unread state after `mark_user_notification_read` succeeds; failures preserve unread state and show a retryable error while still allowing navigation.
- Preference save handling now rechecks pending work after relinquishing `saving`, restarts a worker if a handoff update arrived, and restores/requeues state on failed saves rather than silently dropping the failed patch.

Verification for this slice:
- `nix develop -c rustfmt --edition 2024 packages/default/crates/cf-config/src/config/server.rs packages/default/crates/cf-server/src/api/models.rs packages/default/crates/cf-server/src/handlers/api/user_notifications.rs packages/default/crates/cf-server/src/models/user_notifications.rs packages/default/crates/cf-server/src/queries/user_notifications.rs packages/default/crates/cf-server/src/tasks/user_notification_email.rs packages/web-ui/src/components/layout/topbar.rs packages/web-ui/src/views/profile.rs && git diff --check` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` passed: 4 passed, 4 ignored.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_preference_merge'` passed: 2 passed.
- `node --check checks/web-ui/tests/integration-test.js` passed.
- `nix develop -c bash -c 'cargo test --manifest-path packages/default/crates/cf-config/Cargo.toml server::tests --lib'` passed: 8 passed.

Remaining limitations: requested DB transition tests and the existing DB-backed ignored email tests still require a migrated `CRYSTAL_FORGE_TEST_DATABASE_URL` and were not run in this session; SQLx metadata has not been refreshed; the P2 graceful worker shutdown item remains deferred because the current server background-task wiring does not expose a shutdown signal to pass through without a broader server lifecycle change.

Committed and pushed latest review-fix commit `859e2ecc` (`Guard notification email opt-in`) to branch `TASK-414-account-notifications-sessions` for MR !314. Worktree status after push is clean. Remaining limitations unchanged: DB-backed ignored tests and requested DB transition tests require `CRYSTAL_FORGE_TEST_DATABASE_URL`; SQLx metadata has not been refreshed; MR pipeline status has not been verified in this session; P2 graceful worker shutdown remains deferred pending a broader server shutdown-signal wiring change.

Continued MR !314 P1 review-fix work:
- Centralized notification email URL validation in `cf-config` with `url::Url`, exact loopback host semantics, strict HTTPS public base URL origin checks, and reused it from server capability/worker paths.
- Added runtime token-file readability/non-empty validation to notification email capability reporting.
- Added email delivery claim tokens and compare-and-swap sent/cancel/retry transitions so stale workers cannot overwrite a newer claim result.
- Added category-specific in-app enablement cutoffs to avoid replaying disabled-interval events after in-app/category re-enable.
- Rechecked immediate and digest email category cutoffs at render/send time.
- Allowed first partial weekly digest periods when digest is enabled midweek by selecting users with `weekly_digest_enabled_at < period_end` and rendering/enqueueing from `GREATEST(period_start, weekly_digest_enabled_at)`.
- Fixed failed older notification preference saves to merge under newer pending updates, preserving last action wins.

Verification run in this slice:
- `nix develop -c rustfmt --edition 2024 packages/default/crates/cf-config/src/config/server.rs packages/default/crates/cf-server/src/handlers/api/user_notifications.rs packages/default/crates/cf-server/src/queries/user_notifications.rs packages/default/crates/cf-server/src/tasks/user_notification_email.rs packages/web-ui/src/views/profile.rs && git diff --check` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` passed: 4 passed, 4 ignored.
- `nix develop -c bash -c 'cargo test --manifest-path packages/default/crates/cf-config/Cargo.toml server::tests --lib'` passed: 8 passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_preference_merge'` passed: 2 passed.
- `node --check checks/web-ui/tests/integration-test.js` passed.

Remaining limitations unchanged: DB-backed ignored tests still require `CRYSTAL_FORGE_TEST_DATABASE_URL`; SQLx metadata has not been refreshed in this session; MR pipeline/Nix web-ui check still need final verification.

Committed and pushed MR !314 review-fix commit `46c3177a` (`Fix notification delivery races`) to branch `TASK-414-account-notifications-sessions`. Active task worktree status is clean after push. Remaining limitations unchanged: DB-backed ignored tests require `CRYSTAL_FORGE_TEST_DATABASE_URL`; SQLx metadata has not been refreshed; MR pipeline/Nix web-ui check still need final verification before moving TASK-414 to Review.

Continued MR !314 P1 review-fix work:
- Added migration/user-creation preference initialization so notification defaults exist before the user first opens notification preferences.
- Added default preference insertion to password/dev user creation and external identity user creation transactions.
- Switched notification pagination from timestamp-only cursors to opaque composite `(created_at, id)` cursors, kept legacy `before` compatibility, and return `invalid_notification_cursor` for malformed cursors.
- Updated the web client notification feed model/API call to use string cursors.
- Tightened profile notification preference save rollback to keep a mutable `last_confirmed` snapshot, restore the last successful server state on failed older saves, layer newer optimistic updates, and requeue failed+newer updates so last-action-wins survives out-of-order failures.
- Added focused unit coverage for URL-safe cursor round-tripping and preference save failure/generation behavior.
- Added ignored DB regression tests for pre-API-touch preference initialization and tied-created-at pagination.

Verification for this slice:
- `nix develop -c rustfmt --edition 2024 packages/default/crates/cf-server/src/api/models.rs packages/default/crates/cf-server/src/handlers/api/user_notifications.rs packages/default/crates/cf-server/src/queries/user_notifications.rs packages/default/crates/cf-server/src/queries/users.rs packages/default/crates/cf-server/src/queries/auth_identity.rs packages/web-ui/src/api/client.rs packages/web-ui/src/api/models.rs packages/web-ui/src/views/profile.rs && git diff --check` passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_notifications --lib'` passed: 5 passed, 6 ignored.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo test --manifest-path packages/default/crates/cf-server/Cargo.toml user_sessions --lib'` passed: 3 passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_save'` passed: 3 passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_preference_merge'` passed: 2 passed.
- `node --check checks/web-ui/tests/integration-test.js` passed.

Remaining limitations: DB-backed ignored tests still require a migrated isolated `CRYSTAL_FORGE_TEST_DATABASE_URL`; SQLx metadata was not refreshed; `nix build .#checks.x86_64-linux.web-ui --no-link` was not rerun in this slice. P2 review items remain deferred unless explicitly scoped.

Committed and pushed review-fix commit `244e73d9` (`Fix notification preference and pagination races`) to branch `TASK-414-account-notifications-sessions` for MR !314. Active task worktree and dev integration worktree are clean after push. Remaining limitations are unchanged: DB-backed ignored tests still need a migrated isolated `CRYSTAL_FORGE_TEST_DATABASE_URL`, SQLx metadata has not been refreshed, and the Nix web-ui check was not rerun in this slice.

Continued MR !314 UI review-fix work:
- Added an AppShell-provided `AccountNotificationsContext` for top-bar notification items, unread count, pagination cursor, loading state, loading-more state, and non-destructive error state.
- Refreshed the top-bar notification feed when the bell opens and added `Load more` support using the server `next_cursor`.
- Added `aria-expanded`/`aria-haspopup`, Escape close handling on the panel, and focus restoration to the bell for backdrop/Escape close.
- Added Enter/Space activation for notification rows and changed notification click behavior to close/navigate immediately while marking read asynchronously.
- Changed notification errors to render as a banner without hiding the current feed.
- Added cursor escaping in the web API client.
- Improved active-session rows to render browser/OS labels with relative last-active text and wrapping-friendly layout.
- Added pending/disabled states for individual session revocation and Sign out everywhere confirmation actions.

Verification for this slice:
- `nix develop -c rustfmt --edition 2024 packages/web-ui/src/components/layout/topbar.rs packages/web-ui/src/components/layout/app_shell.rs packages/web-ui/src/components/layout/mod.rs packages/web-ui/src/api/client.rs packages/web-ui/src/views/profile.rs && git diff --check` passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `node --check checks/web-ui/tests/integration-test.js` passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_save'` passed: 3 passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_preference_merge'` passed: 2 passed.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.

A mistaken non-Nix server `cargo check` was run first and failed because `pkg-config`/OpenSSL were unavailable outside the dev shell; it was rerun with `nix develop` successfully. A combined web-ui test filter command also failed because Cargo accepts only one test filter; the two filters were rerun separately and passed.

Continued MR !314 UI review-fix work and pushed commit `58961c1a` (`Finish notification and session UI fixes`) to branch `TASK-414-account-notifications-sessions`.

Implemented in this slice:
- Moved top-bar notification feed state into AppShell-owned `AccountNotificationsContext` so it is account-scoped with the authenticated shell lifecycle.
- Added notification dropdown pagination with a `Load more` control using the server `next_cursor`.
- Added `aria-expanded`/`aria-haspopup`, Escape close, backdrop close with bell focus restoration, and Enter/Space activation for notification rows.
- Preserved notification feed rows when refresh fails by showing errors as a banner.
- Changed notification click/keyboard activation to close and navigate immediately, then mark read asynchronously.
- Escaped the composite cursor delimiter in the web API client query string.
- Improved active-session rows with browser/OS title, relative active time, wrapping-friendly layout, and per-action pending/disabled states for individual revoke and sign-out-everywhere.

Verification run before commit:
- `nix develop -c rustfmt --edition 2024 packages/web-ui/src/components/layout/topbar.rs packages/web-ui/src/components/layout/app_shell.rs packages/web-ui/src/components/layout/mod.rs packages/web-ui/src/api/client.rs packages/web-ui/src/views/profile.rs && git diff --check` passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo check --target wasm32-unknown-unknown'` passed with existing warnings.
- `node --check checks/web-ui/tests/integration-test.js` passed.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_save'` passed: 3 tests.
- `nix develop -c bash -c 'cd packages/web-ui && cargo test --bin crystal-forge-ui notification_preference_merge'` passed: 2 tests.
- `nix develop -c bash -c 'SQLX_OFFLINE=true cargo check --manifest-path packages/default/crates/cf-server/Cargo.toml'` passed with existing warnings.

Post-push status: worktree is clean at `58961c1a`.

Still not verified in this session: DB-backed ignored tests because `CRYSTAL_FORGE_TEST_DATABASE_URL` is unavailable, SQLx offline metadata refresh, MR pipeline, visual baselines/screenshots, and latest `nix build .#checks.x86_64-linux.web-ui --no-link`. Remaining UI integration coverage gaps include mark-all-read, per-item read/navigation, dismiss, incremental loading, keyboard/Escape/focus restoration, preference failures, session revoke/sign-out, and mobile layout.

Latest required Nix web-ui verification after commit `58961c1a`:
- `nix build .#checks.x86_64-linux.web-ui --no-link` passed.

Remaining unverified items: DB-backed ignored tests still require a migrated isolated database via `CRYSTAL_FORGE_TEST_DATABASE_URL`; SQLx metadata has not been refreshed; MR pipeline and visual baselines/screenshots still need confirmation.
<!-- SECTION:NOTES:END -->

## Implementation order

1. Rebase on current `dev` and select the next migration number.
2. Inspect and document the existing session table and all session-creation paths.
3. Add session metadata and revocation support without invalidating existing sessions.
4. Add notification preference and notification-delivery persistence.
5. Add authenticated notification and session APIs.
6. Add canonical attention-to-notification category mapping.
7. Add durable email queue and fake provider.
8. Add weekly digest scheduling.
9. Add AppShell-owned notification state and account-generation guards.
10. Implement the top-bar notification center.
11. Replace the disabled Notifications card.
12. Replace the disabled Active Sessions card.
13. Add server, database, web UI, accessibility, and integration tests.
14. Run the full Nix and CI verification.
15. Update this task with exact commands and results.

## Completion notes

When the task is complete, record:

- migration numbers;
- final API routes;
- final notification category mapping;
- email configuration options;
- digest schedule;
- session retention interval;
- last-seen update interval;
- test commands and results;
- any deferred notification channels;
- screenshots or UI-check references showing parity with the design.
