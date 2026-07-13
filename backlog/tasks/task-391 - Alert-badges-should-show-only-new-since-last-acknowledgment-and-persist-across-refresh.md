---
id: TASK-391
title: >-
  Alert badges should show only "new since last acknowledgment" counts and
  persist dismissal across page refresh
status: Backlog
assignee: []
created_date: '2026-07-13 00:00'
updated_date: '2026-07-13 00:00'
labels:
  - ux
  - alerts
  - sidebar
  - builds
  - evaluations
  - web-ui
dependencies:
  - TASK-385
priority: medium
ordinal: 391000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
## Problem

TASK-385 implemented sidebar/tab attention badges (red pill with a count) for
Systems, Flakes, Environments, Builds, Evaluations, and CVEs. Two properties of
the current design create alert fatigue rather than helping users:

1. **Acknowledgment does not survive a page refresh.** `alerts::acknowledge()`
   writes into an in-memory `GlobalSignal<AlertState>` (`ALERT_STATE`) that is
   wiped whenever the WASM app reloads (browser refresh, re-login, new tab).
   A user visits Builds, sees the Completed tab badge clear, then refreshes
   the page (or comes back later) and the exact same badge reappears with the
   exact same count — even though nothing new has happened.

2. **The badge count is a raw total, not a delta.** The count shown is
   "how many things are currently failing" (e.g. 25 failed builds sitting in
   history), not "how many failures are new since you last looked." A user
   who opens Builds and sees one relevant failure near the top has no way to
   make the badge reflect that they've "seen" the other 24 historical/stale
   failures buried in old rows. The number never meaningfully goes down, so
   users learn to ignore it.

Combined, this produces exactly the outcome alert badges are supposed to
prevent: a nagging, seemingly-permanent indicator that trains users to ignore
it rather than a gentle "something changed since you last checked" hint.

## Desired Outcome

- Visiting/acknowledging a view's failures resets a "last seen" baseline for
  that view that **persists across page refresh, browser restart, and
  re-login** (not just for the current in-memory page load).
- The badge count reflects only items that became failing/new **after** the
  user's last acknowledgment of that view — not the full current total.
- If nothing new has failed since the last acknowledgment, the badge stays
  hidden even after a refresh.
- If something new fails after acknowledgment, the badge reappears showing
  only the new-since-last-check count (e.g. "2" not "27").
- Users are not required to scroll through/review every historical failure to
  clear the badge — visiting the view once is enough to reset the baseline.
- This should apply consistently to every view TASK-385 already wired up
  (Systems, Flakes, Environments, Builds, Evaluations, CVEs).

## Notes

Direct user quote captured during TASK-385 review, describing the desired
mental model:

> they should maybe be like only alerts since last refresh... if things fail
> since last time you loaded or logged in then show the pill with the count
> being the new number of things... then as you go look at them it can
> dismiss it... these should just be gentle notification hints saying
> something here is wrong, not a nagging thing that people go numb to and
> just ignore.

Concrete example given: Builds view shows "25 failed" on the Completed tab
badge. User opens it, sees 1 relevant failure near the top; the other 24 are
old/buried rows the user doesn't care about anymore. Refreshing the page
re-shows the same "25" badge as if nothing had been looked at.

## Scope Hint / Open Architecture Question

This needs a grooming decision before it can be made Sprint-Ready:

- **Client-side (localStorage) baseline** — simplest, matches the original
  design reference's `ackedTabs` localStorage pattern (see
  `docs/design/CrystalForge/components/Shell.jsx`), but is per-browser only
  (a user acknowledging on one device/browser wouldn't clear it on another).
- **Server-side (per-user) baseline** — e.g. a `last_seen_at` timestamp per
  (user, alert_category) pair, persisted in Postgres. More correct for
  multi-device use and matches "since you last logged in" from the user's
  framing, but is a bigger backend lift: new table/columns, new endpoint(s)
  to read/write the baseline, and query changes to compute "count of items
  newer than last_seen_at" per category (needs a reliable timestamp per
  alert item — e.g. build/eval completion time, CVE first-seen time, etc.).

Likely impacted areas if server-side is chosen: new migration, new
`queries`/`handlers::api` module (or extend `navigation.rs` from TASK-385),
`packages/web-ui/src/alerts/mod.rs` (replace/extend the in-memory
`ALERT_STATE.acknowledged` semantics), and each of the six view files wired
up in TASK-385 (`systems_list.rs`, `flakes_list.rs`, `environments_list.rs`,
`builds.rs`, `evaluations.rs`, `cves.rs`).

Not yet scoped: exact acceptance criteria, verification plan, and risk
level — needs grooming to decide the client-vs-server persistence approach
first, since that materially changes the impact area and effort.
<!-- SECTION:DESCRIPTION:END -->
