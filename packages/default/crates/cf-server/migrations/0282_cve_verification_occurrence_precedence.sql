-- A scan can report the same canonical CVE/package for multiple package paths.
-- One whitelisted path does not remove an unwhitelisted affected occurrence.
-- Keep this independent of the service's representative-path ordering so
-- direct verification-item inserts cannot seal a misleading result.
CREATE FUNCTION require_unwhitelisted_cve_verification_precedence()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.result='whitelisted' AND EXISTS (
        SELECT 1 FROM cve_scan_vulnerability_observations observation
        WHERE observation.scan_id=NEW.scan_id
          AND observation.canonical_cve_id=NEW.canonical_cve_id
          AND observation.canonical_package_name=NEW.canonical_package_name
          AND NOT observation.is_whitelisted
    ) THEN
        RAISE EXCEPTION 'A whitelisted CVE result cannot hide an affected occurrence'
            USING ERRCODE='23514',
                  CONSTRAINT='poam_cve_verification_unwhitelisted_precedence';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_z_require_unwhitelisted_cve_verification_precedence
BEFORE INSERT ON poam_cve_verification_items
FOR EACH ROW EXECUTE FUNCTION require_unwhitelisted_cve_verification_precedence();
