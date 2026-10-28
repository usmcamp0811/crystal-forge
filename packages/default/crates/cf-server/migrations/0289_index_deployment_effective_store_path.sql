-- The deployment-status view maps one reported path per active host using
-- COALESCE(store_path, expected_store_path). Existing separate path indexes
-- cannot serve this expression. On an isolated fixture with 1440 derivations
-- and 12 hosts, EXPLAIN showed 1439 rejected derivations per host (17268
-- total); the expression index removed those scans and reduced full list and
-- detail buffer hits from 903 to 537 and 968 to 602, respectively.
CREATE INDEX derivations_deployment_effective_store_path_idx
    ON derivations ((COALESCE(store_path, expected_store_path)));
