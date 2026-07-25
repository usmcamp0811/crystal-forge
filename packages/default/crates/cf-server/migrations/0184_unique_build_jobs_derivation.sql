DELETE FROM build_jobs loser
USING build_jobs winner
WHERE loser.derivation_id = winner.derivation_id
    AND loser.id <> winner.id
    AND (
        loser.created_at > winner.created_at
        OR (loser.created_at = winner.created_at AND loser.id::text > winner.id::text)
    );

CREATE UNIQUE INDEX IF NOT EXISTS idx_build_jobs_derivation_unique
    ON build_jobs (derivation_id);
