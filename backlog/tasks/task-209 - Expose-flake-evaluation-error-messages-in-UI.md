# Task 209: Expose Flake Evaluation Error Messages in UI

**Status**: Backlog  
**Priority**: High  
**Risk**: Low  
**Effort**: Small (2-4 hours)  

---

## Problem

When a flake has evaluation errors, the dashboard health check shows:
> "One or more flakes have evaluation errors on their latest commit. Check flake configuration and commit history."

However, **there is no way to see WHAT the error actually is**. The `commits.evaluation_error_message` column exists in the database but is not exposed through the API or displayed in the UI.

Users are blocked from diagnosing and fixing evaluation failures.

### Suspected Root Cause (User Hypothesis)
The error message mentions "check commit history" which suggests the flake may have had a force push, causing git history issues in the local clone. If true, we may need to:
1. Surface the actual error message to confirm
2. Potentially add a "refresh flake" action to re-clone the repository

---

## Goal

Expose evaluation error messages in the flakes UI so users can diagnose and fix evaluation failures.

---

## Non-Goals

- Implementing automatic retry logic for failed evaluations
- Adding git repository recovery/refresh functionality (this may be needed but should be a separate task based on findings)
- Real-time log streaming for evaluation progress

---

## Acceptance Criteria

1. **API Enhancement**:
   - [ ] `FlakeCommit` model includes `evaluation_error_message: Option<String>`
   - [ ] `fetch_flake_timelines()` query includes `c.evaluation_error_message` in SELECT
   - [ ] Error messages are returned in API responses for commits with evaluation errors

2. **UI Display**:
   - [ ] Flakes list view shows visual indicator (icon/badge) for flakes with evaluation errors on latest commit
   - [ ] Clicking a flake with errors shows the commit timeline
   - [ ] Commits with `evaluation_status = 'failed'` display the error message
   - [ ] Error message is formatted clearly (code block or expandable section)

3. **User Workflow**:
   - [ ] User can identify which flake has errors from the dashboard health check
   - [ ] User can navigate to flakes list and identify the problematic flake
   - [ ] User can read the full error message to diagnose the issue

---

## Technical Approach

### Backend Changes

**File**: `packages/default/src/api/models.rs`
```rust
pub struct FlakeCommit {
    pub id: i32,
    pub hash: String,
    pub message: String,
    pub author: String,
    pub committed_at: DateTime<Utc>,
    pub system_count: i64,
    pub commits_behind: i64,
    pub systems: Vec<String>,
    pub build_status: Option<BuildStatus>,
    #[serde(default)]
    pub evaluation_status: Option<String>,
    #[serde(default)]
    pub evaluation_error_message: Option<String>,  // ADD THIS
}
```

**File**: `packages/default/src/queries/flakes.rs` (line ~479)
```sql
SELECT
    c.id,
    c.git_commit_hash,
    c.commit_timestamp,
    c.message,
    c.author,
    -- ... existing fields ...
    c.evaluation_status,
    c.evaluation_error_message  -- ADD THIS
FROM commits c
-- ... rest of query ...
```

Update the query tuple type and mapping logic to include the error message.

### Frontend Changes

**File**: `packages/web-ui/src/views/flakes_list.rs`

Add visual indicator for flakes with errors:
- Check if latest commit (first in timeline) has `evaluation_status == "failed"`
- Show error icon/badge next to flake name
- Display error message when commit is expanded or in timeline view

**Suggested UI Pattern**:
```
┌─ Flake: my-nixos-config ⚠️ Evaluation Failed ──────┐
│ Latest Commit: abc123 (2 hours ago)                 │
│ ❌ Evaluation Error:                                │
│ ┌──────────────────────────────────────────────┐   │
│ │ error: undefined variable 'pkgs'             │   │
│ │        at /flake.nix:42:5                    │   │
│ └──────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────┘
```

---

## Architectural Constraints

- Error messages may be very long (multi-line Nix error output)
- UI should truncate/collapse long errors with "show more" expansion
- Error messages must not cause XSS vulnerabilities (properly escape HTML)
- Consider that error messages may contain ANSI color codes (strip or render them)

---

## Impact Areas

**Packages Modified**:
- `packages/default/src/api/models.rs` (add field to FlakeCommit)
- `packages/default/src/queries/flakes.rs` (include error_message in query)
- `packages/web-ui/src/api/client.rs` (update FlakeCommit type to match)
- `packages/web-ui/src/views/flakes_list.rs` (display error messages)

**Database**: No migrations needed (column already exists)

**Testing**:
- Create a commit with evaluation_error_message set
- Verify error appears in API response
- Verify error displays in UI

---

## Dependencies

**Blocked by**: None

**Blocks**: Potentially task for "refresh flake clone" if force-push is confirmed as root cause

---

## Verification Plan

### Manual Testing
1. Manually set `evaluation_error_message` on a commit:
   ```sql
   UPDATE commits 
   SET evaluation_status = 'failed',
       evaluation_error_message = 'error: undefined variable ''pkgs'''
   WHERE id = (SELECT id FROM commits ORDER BY commit_timestamp DESC LIMIT 1);
   ```

2. Open flakes list in UI
3. Verify error indicator shows on affected flake
4. Verify error message displays when viewing commit timeline

### Integration Test
- Update `checks/web-ui/tests/integration-test.js` to verify error display
- Or rely on existing flake timeline tests

---

## Out of Scope Discoveries

If during implementation you discover:
- Long error messages causing UI layout issues → create separate task for error message truncation/formatting
- Need for "refresh flake" action → create separate task
- ANSI color code rendering → create separate task for ANSI rendering (nice-to-have)

---

## Notes

- This is a critical observability gap - users cannot diagnose evaluation failures
- The error message already exists in the database, we just need to surface it
- Consider adding this to the dashboard health check details as well (future enhancement)

---

## Related Tasks

- TASK-173: Fix eval logs visibility (related to evaluation observability)
- TASK-160: Create Eval queue view (shows evaluation queue but not errors)
