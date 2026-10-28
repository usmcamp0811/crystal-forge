-- Run after upgrading the fixture database to 0297 with ON_ERROR_STOP=1.
DO $$
DECLARE
  chains bigint;
BEGIN
  SELECT count(*) INTO chains FROM (
    SELECT source_kind,min(human_number) AS n FROM risk_acceptance_source_ids
    GROUP BY source_kind HAVING count(DISTINCT human_number)=1
  ) grouped;
  IF chains<>3 OR (SELECT count(*) FROM risk_acceptance_source_ids)<>7
    OR (SELECT count(DISTINCT human_number) FROM risk_acceptance_source_ids)<>3 THEN
    RAISE EXCEPTION '0296 historical chains did not backfill uniquely';
  END IF;
  IF (SELECT count(*) FROM cve_system_dispositions WHERE retired_at IS NULL)<>1
    OR (SELECT count(*) FROM cve_environment_dispositions WHERE retired_at IS NULL)<>1 THEN
    RAISE EXCEPTION 'renewal changed source active decisions';
  END IF;
END $$;
-- First visibility after upgrade is trigger-owned, even with direct SQL writes.
INSERT INTO cve_system_dispositions(canonical_cve_id,canonical_package_name,system_id,state,justification,accepted_by,accepted_at)
VALUES ('CVE-2099-12345','other-package','00000000-0000-4000-8000-000000000093','accepted','new chain','00000000-0000-4000-8000-000000000091',now());
DO $$
BEGIN
  IF (SELECT count(DISTINCT human_number) FROM risk_acceptance_source_ids)<>4 THEN
    RAISE EXCEPTION 'new decision did not get globally unique RA number';
  END IF;
END $$;
