-- COMPATIBILITY: 0293 already ran on the isolated test database and omitted
-- the exact-CVE link activity kinds introduced in 0259. Keep its history
-- immutable and restore both existing kinds with waiver conversion support.
ALTER TABLE poam_activity DROP CONSTRAINT poam_activity_kind_check;
ALTER TABLE poam_activity ADD CONSTRAINT poam_activity_kind_check CHECK (kind IN (
    'created', 'updated', 'status_changed', 'milestone_added',
    'milestone_updated', 'milestone_removed', 'note', 'finding_linked',
    'finding_unlinked', 'cve_finding_linked', 'cve_finding_unlinked',
    'assignment_linked', 'assignment_unlinked', 'verification_attempted',
    'closed', 'reopened', 'waiver_converted'
));
