---
id: TASK-211
title: Add timezone configuration to user profile or server settings
status: Backlog
created: 2026-03-22
priority: medium
tags: [ui, ux, feature, settings]
---

## Problem

All timestamps displayed in the Crystal Forge UI are currently shown in UTC, which requires users to mentally convert times to their local timezone. This makes it harder to understand when events occurred relative to the user's local time.

Examples of affected timestamps:
- Commit timestamps
- Evaluation started/completed times
- Build started/completed times
- Deployment times
- System heartbeat times

## Goal

Allow users to configure their preferred timezone for displaying timestamps throughout the UI.

## Desired Outcome

Users can:
1. Set their preferred timezone in their user profile settings
2. See all timestamps displayed in their configured timezone
3. See a clear indicator of which timezone is being used (e.g., "2026-03-22 14:30:00 PST" or relative time with timezone hint)

Alternatively (or additionally):
- Server-level default timezone setting that applies to all users who haven't set a personal preference
- Option to toggle between UTC and local time
- Relative time display (e.g., "2 hours ago") with tooltip showing absolute time in user's timezone

## Non-Goals

- Changing how times are stored in the database (they should remain UTC)
- Timezone-aware scheduling or cron features (out of scope for this task)

## Acceptance Criteria

- [ ] User can set timezone preference in their profile settings
- [ ] All timestamp displays throughout the UI respect the user's timezone setting
- [ ] Timezone is clearly indicated in timestamp displays (e.g., suffix, tooltip, or settings indicator)
- [ ] Default behavior when no timezone is set is documented and sensible (UTC or browser-detected)
- [ ] Timezone setting is persisted in user profile database table
- [ ] UI provides timezone selector (dropdown or autocomplete) with common timezones

## Optional Enhancements

- [ ] Auto-detect timezone from browser
- [ ] Show both local time and UTC in tooltips
- [ ] Relative time display ("2 hours ago") with absolute time in tooltip
- [ ] Server-level default timezone configuration

## Implementation Notes

### Frontend (Dioxus)

- Add timezone selection component (dropdown/autocomplete)
- Use a timezone conversion library (e.g., `chrono-tz` in Rust)
- Convert UTC timestamps to user timezone before display
- Consider using `Intl.DateTimeFormat` if using JavaScript interop

### Backend

- Add `timezone` column to `users` table (nullable string, default NULL = UTC)
- Expose timezone in user profile API endpoints
- Include timezone in user session/context

### Database Migration

```sql
ALTER TABLE users ADD COLUMN timezone VARCHAR(64) DEFAULT NULL;
```

### UI Placement Options

1. **User Profile Section** (recommended)
   - Path: `/profile` or `/settings/profile`
   - Grouped with other user preferences
   - Personal to each user

2. **Server Management Section** (alternative for server-wide default)
   - Path: `/admin/settings`
   - Sets default timezone for all users
   - Less flexible but simpler for single-user deployments

## Verification

- [ ] User can select a timezone from profile settings
- [ ] Timezone setting persists across sessions
- [ ] Timestamps in commits view show correct timezone
- [ ] Timestamps in builds view show correct timezone
- [ ] Timestamps in deployments view show correct timezone
- [ ] Timestamps in system health view show correct timezone
- [ ] Timezone is clearly indicated (not ambiguous)

## Related

- Improves UX for users in non-UTC timezones
- Complements existing timestamp displays throughout the UI
- May want to consider i18n/l10n in future for date formatting

## Risk Assessment

**Risk Level:** Low
- Non-breaking change (database column is nullable)
- Frontend-focused feature
- No impact on backend logic or evaluations

## Dependencies

None - can be implemented independently

## Effort Estimate

- Small (2-4 hours for basic implementation)
- Medium (4-8 hours with auto-detect and enhanced UX)
