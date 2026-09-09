ALTER TABLE cve_scans
    ADD COLUMN trigger_source TEXT;

COMMENT ON COLUMN cve_scans.trigger_source IS
    'NULL means legacy or unknown attribution. First-party values currently include manual, post-build, and scheduled. Unknown non-NULL values are preserved for forward compatibility.';
