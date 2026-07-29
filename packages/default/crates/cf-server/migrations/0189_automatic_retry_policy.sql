-- Persist the server-wide automatic retry policy as a singleton.
CREATE TABLE automatic_retry_policy (
    id                     integer PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    max_build_retries      smallint NOT NULL DEFAULT 2
                             CHECK (max_build_retries BETWEEN 0 AND 5),
    max_evaluation_retries smallint NOT NULL DEFAULT 1
                             CHECK (max_evaluation_retries BETWEEN 0 AND 5),
    backoff_seconds        integer NOT NULL DEFAULT 30
                             CHECK (backoff_seconds IN (0, 10, 30, 60, 120, 300)),
    transient_only         boolean NOT NULL DEFAULT true,
    updated_at             timestamptz NOT NULL DEFAULT NOW()
);

INSERT INTO automatic_retry_policy (
    id,
    max_build_retries,
    max_evaluation_retries,
    backoff_seconds,
    transient_only
)
VALUES (1, 2, 1, 30, true);
