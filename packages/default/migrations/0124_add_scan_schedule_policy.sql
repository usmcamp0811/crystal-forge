CREATE TABLE IF NOT EXISTS scan_schedule_policy (
    id integer PRIMARY KEY CHECK (id = 1),
    on_build boolean NOT NULL DEFAULT TRUE,
    deployed_interval varchar(16) NOT NULL DEFAULT '24h',
    recent_interval varchar(16) NOT NULL DEFAULT '24h',
    archived_interval varchar(16) NOT NULL DEFAULT '168h',
    archived_enabled boolean NOT NULL DEFAULT TRUE,
    rebuild_to_scan boolean NOT NULL DEFAULT FALSE,
    updated_at timestamptz NOT NULL DEFAULT NOW(),
    created_at timestamptz NOT NULL DEFAULT NOW()
);

INSERT INTO scan_schedule_policy (
    id,
    on_build,
    deployed_interval,
    recent_interval,
    archived_interval,
    archived_enabled,
    rebuild_to_scan
)
VALUES (1, TRUE, '24h', '24h', '168h', TRUE, FALSE)
ON CONFLICT (id) DO NOTHING;
