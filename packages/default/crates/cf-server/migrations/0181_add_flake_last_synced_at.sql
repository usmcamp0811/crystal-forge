-- Add flakes.last_synced_at: a monotonically-advancing timestamp of the most
-- recent SUCCESSFUL sync completion, distinct from last_sync_at (which is
-- overwritten on every attempt regardless of outcome, including failures).
--
-- Needed to detect whether a successful sync has occurred since an open
-- attention occurrence was last observed. Without this, the attention
-- lifecycle for flakes cannot distinguish "this sync_error/stale_sync
-- occurrence still represents the current, continuous incident" from "a
-- sync succeeded in between (but the attention resolution for that success
-- was lost to a crash), so this occurrence is stale and must not be reused
-- for a later, unrelated failure" -- reusing a stale occurrence silently
-- carries over its original opened_at and any user dismissal onto an
-- unrelated new incident.
ALTER TABLE public.flakes
    ADD COLUMN IF NOT EXISTS last_synced_at timestamptz;

COMMENT ON COLUMN public.flakes.last_synced_at IS
    'Timestamp of the most recent SUCCESSFUL sync completion. Updated only '
    'on success (never on failure), so unlike last_sync_at it is never '
    'clobbered by a later failed attempt. NULL means this flake has never '
    'completed a successful sync.';
