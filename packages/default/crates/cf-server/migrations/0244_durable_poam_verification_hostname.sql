-- Verification history must retain the hostname that identified the system when
-- the server sealed the attempt. Keep the column nullable during a rolling
-- deployment so an older server can still write an item until it is replaced.
ALTER TABLE poam_verification_items
    ADD COLUMN system_hostname text;

-- The existing rows are immutable through a trigger. The migration holds the
-- table lock while it suspends only that trigger, populates the new snapshot,
-- and restores the same protection before commit.
ALTER TABLE poam_verification_items
    DISABLE TRIGGER trigger_prevent_poam_verification_item_mutation;

UPDATE poam_verification_items item
SET system_hostname = system.hostname
FROM systems system
WHERE system.id = item.system_id
  AND item.system_hostname IS NULL;

ALTER TABLE poam_verification_items
    ENABLE TRIGGER trigger_prevent_poam_verification_item_mutation;

COMMENT ON COLUMN poam_verification_items.system_hostname IS
    'Immutable hostname snapshot captured when the verification attempt is written; null only for rolling-version rows that require live-system fallback.';
