-- Conversion history is written into the existing audit stream. Index only
-- committed CVE replacements so the authorized register can resolve a source
-- row to its plan without scanning unrelated audit activity for every row.
CREATE INDEX admin_audit_cve_acceptance_replacements_source_idx
    ON admin_audit_events ((metadata->>'predecessor_id'),
                           (metadata->>'source_type'))
    WHERE action='cve_acceptance_converted';
