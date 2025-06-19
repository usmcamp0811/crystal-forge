CREATE TABLE IF NOT EXISTS tbl_flakes (
    id serial PRIMARY KEY,
    name text NOT NULL,
    repo_url text NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS tbl_commits (
    id serial PRIMARY KEY,
    flake_id int NOT NULL REFERENCES tbl_flakes (id) ON DELETE CASCADE,
    git_commit_hash text NOT NULL,
    commit_timestamp timestamptz NOT NULL,
    UNIQUE (flake_id, git_commit_hash)
);

CREATE TABLE IF NOT EXISTS tbl_evaluation_targets (
    id serial PRIMARY KEY,
    commit_id int NOT NULL REFERENCES tbl_commits (id) ON DELETE CASCADE,
    target_type text NOT NULL,
    target_name text NOT NULL,
    derivation_hash text,
    build_timestamp timestamptz DEFAULT now(),
    UNIQUE (commit_id, target_type, target_name)
);

CREATE TABLE IF NOT EXISTS tbl_system_states (
    id serial PRIMARY KEY,
    hostname text NOT NULL,
    system_derivation_id text NOT NULL,
    context text NOT NULL,
    os text,
    kernel text,
    memory_gb double precision,
    uptime_secs bigint,
    cpu_brand text,
    cpu_cores int,
    board_serial text,
    product_uuid text,
    rootfs_uuid text
);

