-- TASK-440: durable, content-addressed evaluation and flake output snapshots.
--
-- Option payloads are redacted and canonicalized by the server before insert.
-- A payload is stored once by digest and referenced by each configuration
-- snapshot that contains it. Snapshot reads never invoke Nix, Git, or network
-- operations.

ALTER TABLE commits
    ADD COLUMN first_parent_sha text,
    ADD COLUMN first_parent_resolved boolean NOT NULL DEFAULT false,
    ADD COLUMN source_archived boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT commits_first_parent_sha_full_identity
        CHECK (first_parent_sha IS NULL OR first_parent_sha ~ '^[0-9a-f]{40,64}$');

CREATE INDEX commits_flake_first_parent_idx
    ON commits(flake_id, first_parent_sha)
    WHERE first_parent_sha IS NOT NULL;

CREATE TABLE evaluation_option_contents (
    digest bytea PRIMARY KEY CHECK (octet_length(digest) = 32),
    schema_version integer NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    payload jsonb NOT NULL,
    search_text text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE evaluation_snapshots (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE CASCADE,
    configuration_name text NOT NULL CHECK (btrim(configuration_name) <> ''),
    schema_version integer NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    lifecycle text NOT NULL CHECK (
        lifecycle IN ('queued', 'running', 'failed', 'available', 'unavailable')
    ),
    first_parent_sha text,
    error text,
    option_count integer NOT NULL DEFAULT 0 CHECK (option_count >= 0),
    module_count integer NOT NULL DEFAULT 0 CHECK (module_count >= 0),
    evaluation_duration_ms bigint CHECK (evaluation_duration_ms >= 0),
    content_bytes bigint NOT NULL DEFAULT 0 CHECK (content_bytes >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    UNIQUE (commit_id, configuration_name),
    CHECK (first_parent_sha IS NULL OR first_parent_sha ~ '^[0-9a-f]{40,64}$'),
    CHECK (lifecycle = 'failed' OR error IS NULL)
);

CREATE INDEX evaluation_snapshots_commit_configuration_idx
    ON evaluation_snapshots(commit_id, configuration_name);

ALTER TABLE evaluation_snapshots
    ADD CONSTRAINT evaluation_snapshots_id_commit_unique UNIQUE (id, commit_id);
ALTER TABLE derivations
    ADD CONSTRAINT derivations_id_commit_unique UNIQUE (id, commit_id);

CREATE TABLE evaluation_snapshot_options (
    snapshot_id uuid NOT NULL REFERENCES evaluation_snapshots(id) ON DELETE CASCADE,
    option_path text NOT NULL CHECK (btrim(option_path) <> ''),
    content_digest bytea NOT NULL REFERENCES evaluation_option_contents(digest) ON DELETE RESTRICT,
    is_overridden boolean NOT NULL DEFAULT false,
    PRIMARY KEY (snapshot_id, option_path)
);

CREATE INDEX evaluation_snapshot_options_content_idx
    ON evaluation_snapshot_options(content_digest);

CREATE TABLE evaluation_generation_snapshots (
    id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    generation integer NOT NULL CHECK (generation >= 0),
    snapshot_id uuid NOT NULL REFERENCES evaluation_snapshots(id) ON DELETE RESTRICT,
    derivation_id integer NOT NULL REFERENCES derivations(id) ON DELETE RESTRICT,
    commit_id integer NOT NULL REFERENCES commits(id) ON DELETE RESTRICT,
    source_store_path text,
    retained_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (system_id, generation),
    FOREIGN KEY (snapshot_id, commit_id)
        REFERENCES evaluation_snapshots(id, commit_id) ON DELETE RESTRICT,
    FOREIGN KEY (derivation_id, commit_id)
        REFERENCES derivations(id, commit_id) ON DELETE RESTRICT
);

CREATE TABLE flake_output_contents (
    digest bytea PRIMARY KEY CHECK (octet_length(digest) = 32),
    schema_version integer NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE pending_system_deployments
    ADD COLUMN requested_commit_id integer REFERENCES commits(id) ON DELETE SET NULL,
    ADD COLUMN request_identity text,
    ADD COLUMN request_action text;

CREATE INDEX pending_system_deployments_request_identity_idx
    ON pending_system_deployments(system_id, request_identity, issued_at DESC)
    WHERE request_identity IS NOT NULL;

CREATE TABLE deployment_request_reservations (
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE CASCADE,
    request_id uuid NOT NULL UNIQUE,
    requested_commit_id integer NOT NULL REFERENCES commits(id) ON DELETE RESTRICT,
    request_action text NOT NULL CHECK (btrim(request_action) <> ''),
    state text NOT NULL DEFAULT 'reserved' CHECK (
        state IN ('reserved', 'conversion_persisted', 'deploy_failed', 'queued')
    ),
    deployment_id uuid REFERENCES pending_system_deployments(id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (system_id, request_id)
);

-- Preserve explicit identities if this migration is applied over deployment
-- rows created during a rolling upgrade. These rows must conflict before any
-- policy conversion just like newly reserved requests.
INSERT INTO deployment_request_reservations (
    system_id,
    request_id,
    requested_commit_id,
    request_action,
    state,
    deployment_id
)
SELECT
    system_id,
    substring(request_identity FROM 10)::uuid,
    requested_commit_id,
    request_action,
    CASE WHEN status = 'pending' THEN 'queued' ELSE 'deploy_failed' END,
    id
FROM pending_system_deployments
WHERE request_identity ~ '^explicit:[0-9a-fA-F-]{36}$'
  AND requested_commit_id IS NOT NULL
  AND request_action IS NOT NULL
ON CONFLICT (request_id) DO NOTHING;

CREATE FUNCTION preserve_deployment_request_reservation_intent()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.system_id IS DISTINCT FROM OLD.system_id
       OR NEW.request_id IS DISTINCT FROM OLD.request_id
       OR NEW.requested_commit_id IS DISTINCT FROM OLD.requested_commit_id
       OR NEW.request_action IS DISTINCT FROM OLD.request_action THEN
        RAISE EXCEPTION 'explicit deployment request intent is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER deployment_request_reservation_intent_immutable
BEFORE UPDATE OF system_id, request_id, requested_commit_id, request_action
ON deployment_request_reservations
FOR EACH ROW
EXECUTE FUNCTION preserve_deployment_request_reservation_intent();

CREATE FUNCTION preserve_pending_deployment_identity()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.requested_commit_id IS NOT NULL
       AND NEW.requested_commit_id IS DISTINCT FROM OLD.requested_commit_id THEN
        RAISE EXCEPTION 'requested deployment commit identity is immutable';
    END IF;
    IF OLD.request_identity IS NOT NULL
       AND NEW.request_identity IS DISTINCT FROM OLD.request_identity THEN
        RAISE EXCEPTION 'deployment request identity is immutable';
    END IF;
    IF OLD.request_action IS NOT NULL
       AND NEW.request_action IS DISTINCT FROM OLD.request_action THEN
        RAISE EXCEPTION 'deployment request action is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER pending_system_deployment_identity_immutable
BEFORE UPDATE OF requested_commit_id, request_identity, request_action
ON pending_system_deployments
FOR EACH ROW
EXECUTE FUNCTION preserve_pending_deployment_identity();

CREATE TABLE flake_output_snapshots (
    commit_id integer PRIMARY KEY REFERENCES commits(id) ON DELETE CASCADE,
    lifecycle text NOT NULL CHECK (
        lifecycle IN ('queued', 'running', 'failed', 'available', 'unavailable')
    ),
    schema_version integer NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    first_parent_sha text,
    content_digest bytea REFERENCES flake_output_contents(digest) ON DELETE RESTRICT,
    error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    CHECK (first_parent_sha IS NULL OR first_parent_sha ~ '^[0-9a-f]{40,64}$'),
    CHECK (lifecycle <> 'available' OR content_digest IS NOT NULL),
    CHECK (lifecycle = 'failed' OR error IS NULL)
);

COMMENT ON TABLE evaluation_option_contents IS
    'Content-addressed, pre-persistence-redacted option payloads shared by evaluation snapshots.';
COMMENT ON COLUMN pending_system_deployments.request_identity IS
    'Explicit durable retry identity or stable derived legacy intent. Legacy identities replay terminal results for at least 24 hours; later calls can intentionally redeploy.';
COMMENT ON COLUMN pending_system_deployments.request_action IS
    'Immutable deployment action bound to request_identity for conflict detection.';
COMMENT ON TABLE deployment_request_reservations IS
    'Durably reserves an explicit request intent before policy conversion. State records conversion-persisted and deploy-failed partial success for safe retries.';
COMMENT ON TABLE evaluation_generation_snapshots IS
    'Retained generation references. RESTRICT prevents snapshot deletion while a generation is retained.';
COMMENT ON TABLE flake_output_snapshots IS
    'Database-only revision output read model keyed by the full commit identity.';
COMMENT ON COLUMN commits.first_parent_sha IS
    'Full first-parent identity extracted from authoritative Git history; NULL only for a root or history not yet synchronized by Git.';
COMMENT ON COLUMN commits.first_parent_resolved IS
    'True when Git supplied authoritative parent data. A true value with NULL first_parent_sha identifies a root commit; false means unknown.';
COMMENT ON COLUMN commits.source_archived IS
    'True for retained generation history from a replaced flake source. Archived commits remain generation-queryable but are not active revisions of the current source.';
