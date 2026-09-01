-- Dependency build-plan counts describe source builds, not closure presence.
-- Existing rows predate this calculation and therefore remain unavailable.
ALTER TABLE derivations
    ADD COLUMN dependency_build_count integer,
    ADD COLUMN dependency_build_plan_status text NOT NULL DEFAULT 'unavailable';

-- Legacy closure counts include non-derivation store paths and cannot satisfy
-- the dependency-derivation contract. A new calculation repopulates the total.
UPDATE derivations
SET closure_total = NULL,
    closure_cached = NULL;

ALTER TABLE derivations
    ADD CONSTRAINT derivations_closure_total_nonnegative
        CHECK (closure_total IS NULL OR closure_total >= 0),
    ADD CONSTRAINT derivations_dependency_build_count_nonnegative
        CHECK (dependency_build_count IS NULL OR dependency_build_count >= 0),
    ADD CONSTRAINT derivations_dependency_build_plan_status_valid
        CHECK (
            dependency_build_plan_status IN (
                'unavailable',
                'calculating',
                'complete',
                'failed'
            )
        ),
    ADD CONSTRAINT derivations_dependency_build_plan_complete_count
        CHECK (
            (dependency_build_plan_status = 'complete')
            = (dependency_build_count IS NOT NULL)
        ),
    ADD CONSTRAINT derivations_dependency_build_plan_complete_total
        CHECK (
            dependency_build_plan_status != 'complete'
            OR closure_total IS NOT NULL
        ),
    ADD CONSTRAINT derivations_dependency_build_count_within_total
        CHECK (
            dependency_build_count IS NULL
            OR dependency_build_count <= closure_total
        );
