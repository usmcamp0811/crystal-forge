-- Server-owned build failure authority must not be inferred from builder logs.

ALTER TABLE build_jobs
    ADD COLUMN server_failure_code text;

ALTER TABLE build_jobs
    ADD CONSTRAINT build_jobs_server_failure_code_check CHECK (
        server_failure_code IS NULL
        OR (
            status = 'failed'
            AND server_failure_code = 'evaluator_contract_obsolete'
        )
    );

COMMENT ON COLUMN build_jobs.server_failure_code IS
    'Server-owned terminal failure code. Builders cannot set this field. evaluator_contract_obsolete authorizes same-row revival only after authoritative reevaluation republishes the derivation.';
