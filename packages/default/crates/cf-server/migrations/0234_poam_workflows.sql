-- Stable findings and Plans of Action and Milestones (POA&M).
-- Finding identity is deliberately independent of assessment/version context.

CREATE SEQUENCE poam_human_id_seq;

CREATE TABLE poam_findings (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    system_id uuid NOT NULL REFERENCES systems(id) ON DELETE RESTRICT,
    policy_lineage_id uuid NOT NULL REFERENCES deployment_policies(id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (system_id, policy_lineage_id),
    UNIQUE (id, system_id, policy_lineage_id)
);

CREATE OR REPLACE FUNCTION prevent_poam_finding_identity_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.system_id <> OLD.system_id OR NEW.policy_lineage_id <> OLD.policy_lineage_id THEN
        RAISE EXCEPTION 'POA&M finding identity is immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_prevent_poam_finding_identity_mutation
    BEFORE UPDATE OF system_id, policy_lineage_id ON poam_findings
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_finding_identity_mutation();

INSERT INTO poam_findings (system_id, policy_lineage_id)
SELECT DISTINCT system_id, policy_lineage_id
FROM composite_policy_assessments
ON CONFLICT (system_id, policy_lineage_id) DO NOTHING;

CREATE TABLE poams (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    human_number bigint NOT NULL DEFAULT nextval('poam_human_id_seq') UNIQUE,
    title text NOT NULL CHECK (btrim(title) <> ''),
    plan text NOT NULL DEFAULT '',
    owner text NOT NULL DEFAULT '',
    target_date date,
    risk text NOT NULL CHECK (risk IN ('high', 'medium', 'low')),
    status text NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'in_progress', 'blocked', 'awaiting_verification', 'completed')),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    closed_at timestamptz,
    closure_attempt_id uuid,
    CHECK ((status = 'completed') = (closed_at IS NOT NULL)),
    CHECK (status <> 'completed' OR closure_attempt_id IS NOT NULL)
);

ALTER SEQUENCE poam_human_id_seq OWNED BY poams.human_number;

CREATE TABLE poam_finding_links (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    poam_id uuid NOT NULL REFERENCES poams(id) ON DELETE CASCADE,
    finding_id uuid NOT NULL REFERENCES poam_findings(id) ON DELETE RESTRICT,
    linked_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    linked_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    retired_at timestamptz,
    retired_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    retirement_reason text,
    CHECK ((retired_at IS NULL) = (retired_by IS NULL)),
    CHECK (retired_at IS NULL OR btrim(retirement_reason) <> '')
);

CREATE UNIQUE INDEX poam_finding_links_one_active_remediation
    ON poam_finding_links (finding_id) WHERE retired_at IS NULL;
CREATE UNIQUE INDEX poam_finding_links_one_active_pair
    ON poam_finding_links (poam_id, finding_id) WHERE retired_at IS NULL;

-- The parent and link are inserted in one transaction, so enforce this at
-- commit rather than weakening the invariant during POA&M creation.
CREATE OR REPLACE FUNCTION require_active_poam_finding()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_poam_id uuid;
BEGIN
    IF TG_TABLE_NAME = 'poams' THEN
        v_poam_id := COALESCE(
            (to_jsonb(NEW)->>'id')::uuid,
            (to_jsonb(OLD)->>'id')::uuid
        );
    ELSE
        v_poam_id := COALESCE(
            (to_jsonb(NEW)->>'poam_id')::uuid,
            (to_jsonb(OLD)->>'poam_id')::uuid
        );
    END IF;
    IF EXISTS (SELECT 1 FROM poams WHERE id = v_poam_id AND status <> 'completed')
       AND NOT EXISTS (
           SELECT 1 FROM poam_finding_links
           WHERE poam_id = v_poam_id AND retired_at IS NULL
       ) THEN
        RAISE EXCEPTION 'A non-completed POA&M requires an active finding'
            USING ERRCODE = '23514', CONSTRAINT = 'poams_active_finding_required';
    END IF;
    IF EXISTS (SELECT 1 FROM poams WHERE id = v_poam_id AND status = 'completed')
       AND EXISTS (
           SELECT 1 FROM poam_finding_links
           WHERE poam_id = v_poam_id AND retired_at IS NULL
       ) THEN
        RAISE EXCEPTION 'A completed POA&M cannot have an active finding'
            USING ERRCODE = '23514', CONSTRAINT = 'poams_completed_without_active_finding';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_poams_require_active_finding
    AFTER INSERT OR UPDATE ON poams DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION require_active_poam_finding();
CREATE CONSTRAINT TRIGGER trigger_poam_links_require_active_finding
    AFTER INSERT OR UPDATE OR DELETE ON poam_finding_links DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION require_active_poam_finding();

CREATE TABLE poam_assignment_references (
    poam_id uuid NOT NULL REFERENCES poams(id) ON DELETE CASCADE,
    assignment_id uuid NOT NULL REFERENCES compliance_bundle_assignments(id) ON DELETE RESTRICT,
    assignment_version_id uuid NOT NULL
        REFERENCES compliance_bundle_assignment_versions(id) ON DELETE RESTRICT,
    added_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    added_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (poam_id, assignment_version_id),
    CONSTRAINT poam_assignment_references_version_lineage_fk
        FOREIGN KEY (assignment_version_id, assignment_id)
        REFERENCES compliance_bundle_assignment_versions (id, assignment_id)
        ON DELETE RESTRICT
);

CREATE TABLE poam_milestones (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    poam_id uuid NOT NULL REFERENCES poams(id) ON DELETE CASCADE,
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    title text NOT NULL CHECK (btrim(title) <> ''),
    target_date date NOT NULL,
    completed_at timestamptz,
    completed_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    created_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    updated_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((completed_at IS NULL) = (completed_by IS NULL)),
    UNIQUE (poam_id, ordinal)
);

CREATE TABLE poam_activity (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    poam_id uuid NOT NULL REFERENCES poams(id) ON DELETE CASCADE,
    actor_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    kind text NOT NULL CHECK (kind IN (
        'created', 'updated', 'status_changed', 'milestone_added',
        'milestone_updated', 'milestone_removed', 'note', 'finding_linked',
        'finding_unlinked', 'assignment_linked', 'assignment_unlinked',
        'verification_attempted', 'closed', 'reopened'
    )),
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE finding_waivers (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    finding_id uuid NOT NULL REFERENCES poam_findings(id) ON DELETE RESTRICT,
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected', 'expired', 'revoked')),
    justification text NOT NULL CHECK (btrim(justification) <> ''),
    -- Source identifiers are non-enforcing snapshots. Phase-4 assessment rows
    -- remain replaceable and the immutable observation token is authoritative.
    policy_version_id uuid NOT NULL,
    assessment_id uuid NOT NULL,
    observation_token text NOT NULL CHECK (btrim(observation_token) <> ''),
    observation_snapshot jsonb NOT NULL,
    accepted_by uuid REFERENCES users(id) ON DELETE RESTRICT,
    accepted_at timestamptz,
    expires_at timestamptz,
    created_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((status IN ('accepted', 'revoked', 'expired')
            AND accepted_by IS NOT NULL AND accepted_at IS NOT NULL)
        OR (status IN ('pending', 'rejected')
            AND accepted_by IS NULL AND accepted_at IS NULL)),
    UNIQUE (id, finding_id)
);

CREATE UNIQUE INDEX finding_waivers_one_accepted
    ON finding_waivers (finding_id) WHERE status = 'accepted';

CREATE TABLE finding_waiver_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    waiver_id uuid NOT NULL REFERENCES finding_waivers(id) ON DELETE RESTRICT,
    actor_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    from_status text,
    to_status text NOT NULL CHECK (to_status IN ('pending','accepted','rejected','expired','revoked')),
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX finding_waiver_events_waiver_created_idx
    ON finding_waiver_events (waiver_id, created_at, id);

CREATE TABLE poam_verification_attempts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    poam_id uuid NOT NULL REFERENCES poams(id) ON DELETE RESTRICT,
    attempted_by uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    outcome text NOT NULL CHECK (outcome IN ('accepted', 'rejected')),
    poam_revision bigint NOT NULL,
    attempted_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    sealed_at timestamptz,
    UNIQUE (id, poam_id)
);

CREATE TABLE poam_verification_items (
    attempt_id uuid NOT NULL REFERENCES poam_verification_attempts(id) ON DELETE RESTRICT,
    finding_id uuid NOT NULL,
    system_id uuid NOT NULL,
    policy_lineage_id uuid NOT NULL,
    result text NOT NULL CHECK (result IN (
        'pass', 'waiver', 'missing', 'stale', 'fail', 'error', 'unknown',
        'not_checked', 'warn', 'not_applicable'
    )),
    policy_version_id uuid,
    assessment_id uuid,
    derivation_id integer,
    target_store_path text,
    effective_set_digest text,
    effective_config_digest text,
    effective_config jsonb,
    observed_outcome text,
    observation_token text,
    observation_snapshot jsonb,
    assessment_updated_at timestamptz,
    bundle_ids uuid[] NOT NULL DEFAULT '{}',
    bundle_version_ids uuid[] NOT NULL DEFAULT '{}',
    requirement_version_ids uuid[] NOT NULL DEFAULT '{}',
    waiver_id uuid,
    observed_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    detail text NOT NULL,
    PRIMARY KEY (attempt_id, finding_id),
    CHECK (result NOT IN ('pass','waiver') OR (assessment_id IS NOT NULL AND policy_version_id IS NOT NULL
        AND derivation_id IS NOT NULL AND target_store_path IS NOT NULL
        AND effective_set_digest IS NOT NULL AND effective_config_digest IS NOT NULL
        AND effective_config IS NOT NULL AND observed_outcome IS NOT NULL
        AND observation_token IS NOT NULL AND observation_snapshot IS NOT NULL
        AND assessment_updated_at IS NOT NULL)),
    CHECK ((assessment_id IS NULL AND observation_snapshot IS NULL AND assessment_updated_at IS NULL)
        OR (assessment_id IS NOT NULL AND observation_snapshot IS NOT NULL AND assessment_updated_at IS NOT NULL)),
    CHECK ((result = 'waiver') = (waiver_id IS NOT NULL)),
    FOREIGN KEY (finding_id, system_id, policy_lineage_id)
        REFERENCES poam_findings(id, system_id, policy_lineage_id) ON DELETE RESTRICT,
    FOREIGN KEY (waiver_id, finding_id) REFERENCES finding_waivers(id, finding_id) ON DELETE RESTRICT
);

CREATE OR REPLACE FUNCTION require_unsealed_verification_attempt()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_sealed_at timestamptz;
BEGIN
    SELECT sealed_at INTO v_sealed_at
    FROM poam_verification_attempts
    WHERE id=NEW.attempt_id
    FOR UPDATE;
    IF v_sealed_at IS NOT NULL THEN
        RAISE EXCEPTION 'Sealed POA&M verification evidence is immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_require_unsealed_verification_attempt
    BEFORE INSERT ON poam_verification_items
    FOR EACH ROW EXECUTE FUNCTION require_unsealed_verification_attempt();

ALTER TABLE poams
    ADD CONSTRAINT poams_closure_attempt_fk
    FOREIGN KEY (closure_attempt_id, id) REFERENCES poam_verification_attempts(id, poam_id) ON DELETE RESTRICT
    DEFERRABLE INITIALLY DEFERRED;

CREATE OR REPLACE FUNCTION validate_poam_closure_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status = 'completed' AND (
        NOT EXISTS (
            SELECT 1 FROM poam_verification_attempts attempt
            WHERE attempt.id=NEW.closure_attempt_id AND attempt.poam_id=NEW.id
              AND attempt.outcome='accepted' AND attempt.sealed_at IS NOT NULL
        )
        OR NOT EXISTS (
            SELECT 1 FROM poam_verification_items item
            WHERE item.attempt_id=NEW.closure_attempt_id
        )
        OR EXISTS (
            (SELECT link.finding_id FROM poam_finding_links link
             WHERE link.poam_id=NEW.id
               AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
            EXCEPT
            (SELECT item.finding_id FROM poam_verification_items item
             WHERE item.attempt_id=NEW.closure_attempt_id)
        )
        OR EXISTS (
            (SELECT item.finding_id FROM poam_verification_items item
             WHERE item.attempt_id=NEW.closure_attempt_id)
            EXCEPT
            (SELECT link.finding_id FROM poam_finding_links link
             WHERE link.poam_id=NEW.id
                AND link.retirement_reason='closed:'||NEW.closure_attempt_id::text)
        )
        OR EXISTS (
            SELECT 1 FROM poam_verification_items item
            WHERE item.attempt_id=NEW.closure_attempt_id
              AND NOT (
                (item.result='pass' AND item.observed_outcome='pass' AND item.waiver_id IS NULL
                 AND item.observation_snapshot->'assessment'->>'id'=item.assessment_id::text
                 AND item.observation_snapshot->'assessment'->>'system_id'=item.system_id::text
                 AND item.observation_snapshot->'assessment'->>'policy_lineage_id'=item.policy_lineage_id::text
                 AND item.observation_snapshot->'assessment'->>'policy_version_id'=item.policy_version_id::text
                 AND item.observation_snapshot->'assessment'->>'overall_outcome'='pass')
                OR
                (item.result='waiver' AND item.observed_outcome='fail'
                 AND item.observation_snapshot->'assessment'->>'overall_outcome'='fail'
                 AND EXISTS (
                    SELECT 1 FROM finding_waivers waiver
                    WHERE waiver.id=item.waiver_id AND waiver.finding_id=item.finding_id
                      AND waiver.assessment_id=item.assessment_id
                      AND waiver.policy_version_id=item.policy_version_id
                      AND waiver.observation_token=item.observation_token
                      AND waiver.observation_snapshot=item.observation_snapshot
                      AND waiver.status='accepted' AND waiver.accepted_at<=CURRENT_TIMESTAMP
                      AND (waiver.expires_at IS NULL OR waiver.expires_at>CURRENT_TIMESTAMP)
                 ))
              )
        )
    ) THEN
        RAISE EXCEPTION 'Completed POA&M closure evidence is incomplete or inconsistent'
            USING ERRCODE='23514', CONSTRAINT='poams_authoritative_closure_evidence';
    END IF;
    RETURN NEW;
END;
$$;

CREATE CONSTRAINT TRIGGER trigger_validate_poam_closure_evidence
    AFTER INSERT OR UPDATE OF status, closure_attempt_id ON poams
    DEFERRABLE INITIALLY DEFERRED FOR EACH ROW
    EXECUTE FUNCTION validate_poam_closure_evidence();

CREATE OR REPLACE FUNCTION protect_poam_finding_link_history()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        RAISE EXCEPTION 'POA&M finding link history is immutable';
    END IF;
    IF NEW.id<>OLD.id OR NEW.poam_id<>OLD.poam_id OR NEW.finding_id<>OLD.finding_id
       OR NEW.linked_by<>OLD.linked_by OR NEW.linked_at<>OLD.linked_at THEN
        RAISE EXCEPTION 'POA&M finding link identity is immutable';
    END IF;
    IF OLD.retired_at IS NOT NULL THEN
        RAISE EXCEPTION 'Retired POA&M finding links are immutable';
    END IF;
    IF NEW.retired_at IS NULL OR NEW.retired_by IS NULL
       OR btrim(COALESCE(NEW.retirement_reason,''))='' THEN
        RAISE EXCEPTION 'A POA&M finding link update must retire the active link';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_poam_finding_link_history
    BEFORE UPDATE OR DELETE ON poam_finding_links
    FOR EACH ROW EXECUTE FUNCTION protect_poam_finding_link_history();

CREATE VIEW poam_current_finding_links AS
SELECT link.*
FROM poam_finding_links link
JOIN poams poam ON poam.id=link.poam_id
WHERE (poam.status<>'completed' AND link.retired_at IS NULL)
   OR (poam.status='completed'
       AND link.retirement_reason='closed:'||poam.closure_attempt_id::text);

CREATE VIEW poam_context_systems AS
SELECT link.poam_id,finding.system_id
FROM poam_finding_links link
JOIN poam_findings finding ON finding.id=link.finding_id
UNION
SELECT reference.poam_id,system.id
FROM poam_assignment_references reference
JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
JOIN systems system ON system.id=assignment.system_id
   OR system.environment_id=assignment.environment_id;

CREATE FUNCTION poam_visible_to_environments(v_poam_id uuid,v_environment_ids uuid[])
RETURNS boolean LANGUAGE sql STABLE AS $$
SELECT (EXISTS (SELECT 1 FROM poam_current_finding_links WHERE poam_id=v_poam_id)
        OR EXISTS (SELECT 1 FROM poam_assignment_references WHERE poam_id=v_poam_id))
  AND NOT EXISTS (
    SELECT 1 FROM poam_context_systems context
    JOIN systems system ON system.id=context.system_id
    WHERE context.poam_id=v_poam_id
      AND (system.environment_id IS NULL OR NOT(system.environment_id=ANY(v_environment_ids))))
  AND NOT EXISTS (
    SELECT 1 FROM poam_assignment_references reference
    JOIN compliance_bundle_assignment_versions version ON version.id=reference.assignment_version_id
    JOIN compliance_bundle_assignments assignment ON assignment.id=version.assignment_id
    LEFT JOIN systems assigned_system ON assigned_system.id=assignment.system_id
    WHERE reference.poam_id=v_poam_id
      AND (COALESCE(assignment.environment_id,assigned_system.environment_id) IS NULL
        OR NOT(COALESCE(assignment.environment_id,assigned_system.environment_id)=ANY(v_environment_ids))));
$$;

CREATE INDEX poams_status_target_idx ON poams (status, target_date, human_number);
CREATE INDEX poams_search_idx ON poams USING gin (to_tsvector('simple',
    coalesce(title, '') || ' ' || coalesce(plan, '') || ' ' || coalesce(owner, '')));
CREATE INDEX poam_findings_system_policy_idx ON poam_findings (system_id, policy_lineage_id);
CREATE INDEX poam_links_poam_active_idx ON poam_finding_links (poam_id, finding_id) WHERE retired_at IS NULL;
CREATE INDEX poam_activity_poam_created_idx ON poam_activity (poam_id, created_at DESC, id);
CREATE INDEX poam_milestones_poam_idx ON poam_milestones (poam_id, ordinal);
CREATE INDEX poam_assignment_refs_version_idx ON poam_assignment_references (assignment_version_id, poam_id);
CREATE INDEX poam_verification_attempts_poam_idx ON poam_verification_attempts (poam_id, attempted_at DESC);
CREATE INDEX finding_waivers_finding_status_idx ON finding_waivers (finding_id, status, expires_at);
CREATE INDEX poam_links_poam_retirement_idx
    ON poam_finding_links (poam_id, retirement_reason, finding_id) WHERE retired_at IS NOT NULL;
CREATE INDEX composite_policy_assessments_current_finding_idx
    ON composite_policy_assessments (
        system_id, policy_lineage_id, target_store_path, updated_at DESC, id DESC
    );
CREATE INDEX policy_requirement_mappings_policy_requirement_idx
    ON policy_requirement_mappings (policy_version_id, requirement_version_id);

-- Every writer of composite assessment state takes the same stable-finding
-- lock used by closure. This prevents a close from racing a superseding Fail.
CREATE OR REPLACE FUNCTION lock_poam_finding_key(v_system_id uuid, v_policy_id uuid)
RETURNS void LANGUAGE sql AS $$
    SELECT pg_advisory_xact_lock(hashtextextended(v_system_id::text || ':' || v_policy_id::text, 433));
$$;

CREATE OR REPLACE FUNCTION try_lock_poam_finding_key(v_system_id uuid, v_policy_id uuid)
RETURNS void LANGUAGE plpgsql AS $$
BEGIN
    IF NOT pg_try_advisory_xact_lock(hashtextextended(v_system_id::text || ':' || v_policy_id::text, 433)) THEN
        RAISE EXCEPTION 'POA&M finding state changed concurrently' USING ERRCODE='40001';
    END IF;
END;
$$;

CREATE OR REPLACE FUNCTION lock_and_materialize_poam_finding()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_system_id uuid;
    v_policy_id uuid;
BEGIN
    v_system_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.system_id ELSE NEW.system_id END;
    v_policy_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.policy_lineage_id ELSE NEW.policy_lineage_id END;
    PERFORM try_lock_poam_finding_key(v_system_id, v_policy_id);
    IF TG_OP <> 'DELETE' THEN
        INSERT INTO poam_findings (system_id, policy_lineage_id)
        VALUES (NEW.system_id, NEW.policy_lineage_id)
        ON CONFLICT (system_id, policy_lineage_id) DO NOTHING;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_composite_assessment_poam_finding_lock
    BEFORE INSERT OR UPDATE OR DELETE ON composite_policy_assessments
    FOR EACH ROW EXECUTE FUNCTION lock_and_materialize_poam_finding();

CREATE OR REPLACE FUNCTION lock_poam_findings_for_system_state()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_key record;
BEGIN
    FOR v_key IN
        SELECT f.system_id, f.policy_lineage_id
        FROM systems s JOIN poam_findings f ON f.system_id = s.id
        WHERE s.hostname IN (
            CASE WHEN TG_OP = 'INSERT' THEN NEW.hostname ELSE OLD.hostname END,
            CASE WHEN TG_OP = 'DELETE' THEN OLD.hostname ELSE NEW.hostname END
        )
        ORDER BY f.system_id, f.policy_lineage_id
    LOOP
        IF TG_OP='INSERT' THEN
            -- system_states is append-only on the reporting path, so waiting
            -- here cannot invert against a row lock needed by verification.
            PERFORM lock_poam_finding_key(v_key.system_id, v_key.policy_lineage_id);
        ELSE
            PERFORM try_lock_poam_finding_key(v_key.system_id, v_key.policy_lineage_id);
        END IF;
    END LOOP;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_system_state_poam_finding_lock
    BEFORE INSERT OR UPDATE OR DELETE ON system_states
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_system_state();

CREATE OR REPLACE FUNCTION lock_poam_findings_for_system_metadata()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_key record;
BEGIN
    FOR v_key IN
        SELECT system_id, policy_lineage_id
        FROM poam_findings
        WHERE system_id IN (OLD.id, NEW.id)
        ORDER BY system_id, policy_lineage_id
    LOOP
        PERFORM try_lock_poam_finding_key(v_key.system_id, v_key.policy_lineage_id);
    END LOOP;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_system_metadata_poam_finding_lock
    BEFORE UPDATE OF hostname, environment_id ON systems
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_system_metadata();

CREATE OR REPLACE FUNCTION prevent_poam_finding_link_reparenting()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.poam_id <> OLD.poam_id THEN
        RAISE EXCEPTION 'A POA&M finding link cannot be moved between POA&Ms'
            USING ERRCODE = '23514', CONSTRAINT = 'poam_finding_link_poam_immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_prevent_poam_finding_link_reparenting
    BEFORE UPDATE OF poam_id ON poam_finding_links
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_finding_link_reparenting();

CREATE OR REPLACE FUNCTION lock_poam_findings_for_policy()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_policy_id uuid;
    v_key record;
BEGIN
    v_policy_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END;
    FOR v_key IN
        SELECT system_id, policy_lineage_id FROM poam_findings
        WHERE policy_lineage_id = v_policy_id ORDER BY system_id, policy_lineage_id
    LOOP
        PERFORM try_lock_poam_finding_key(v_key.system_id, v_key.policy_lineage_id);
    END LOOP;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_deployment_policy_poam_finding_lock
    BEFORE UPDATE OR DELETE ON deployment_policies
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_policy();

CREATE OR REPLACE FUNCTION lock_poam_findings_for_policy_version()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_policy_id uuid;
    v_key record;
BEGIN
    v_policy_id := CASE WHEN TG_OP='DELETE' THEN OLD.policy_id ELSE NEW.policy_id END;
    FOR v_key IN
        SELECT system_id,policy_lineage_id FROM poam_findings
        WHERE policy_lineage_id=v_policy_id ORDER BY system_id,policy_lineage_id
    LOOP
        PERFORM try_lock_poam_finding_key(v_key.system_id,v_key.policy_lineage_id);
    END LOOP;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_deployment_policy_version_poam_finding_lock
    BEFORE INSERT OR UPDATE OR DELETE ON deployment_policy_versions
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_policy_version();

CREATE OR REPLACE FUNCTION lock_poam_findings_for_assignment()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_assignment_id uuid;
    v_key record;
BEGIN
    v_assignment_id := CASE
        WHEN TG_TABLE_NAME = 'compliance_bundle_assignments' THEN
            COALESCE((to_jsonb(NEW)->>'id')::uuid, (to_jsonb(OLD)->>'id')::uuid)
        ELSE COALESCE((to_jsonb(NEW)->>'assignment_id')::uuid, (to_jsonb(OLD)->>'assignment_id')::uuid)
    END;
    FOR v_key IN
        SELECT DISTINCT f.system_id, f.policy_lineage_id
        FROM systems s
        JOIN poam_findings f ON f.system_id = s.id
        WHERE s.id = COALESCE((to_jsonb(NEW)->>'system_id')::uuid,(to_jsonb(OLD)->>'system_id')::uuid)
           OR s.id = (to_jsonb(OLD)->>'system_id')::uuid
           OR s.environment_id = COALESCE((to_jsonb(NEW)->>'environment_id')::uuid,(to_jsonb(OLD)->>'environment_id')::uuid)
           OR s.environment_id = (to_jsonb(OLD)->>'environment_id')::uuid
           OR EXISTS (SELECT 1 FROM compliance_bundle_assignments a
                      WHERE a.id=v_assignment_id AND (s.id=a.system_id OR s.environment_id=a.environment_id))
        ORDER BY f.system_id, f.policy_lineage_id
    LOOP
        PERFORM try_lock_poam_finding_key(v_key.system_id, v_key.policy_lineage_id);
    END LOOP;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_assignment_poam_finding_lock
    BEFORE UPDATE OR DELETE ON compliance_bundle_assignments
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_assignment();
CREATE TRIGGER trigger_assignment_version_poam_finding_lock
    BEFORE INSERT ON compliance_bundle_assignment_versions
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_assignment();
CREATE TRIGGER trigger_assignment_addition_poam_finding_lock
    BEFORE INSERT ON compliance_assignment_additions
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_assignment();
CREATE TRIGGER trigger_assignment_exclusion_poam_finding_lock
    BEFORE INSERT ON compliance_assignment_exclusions
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_assignment();
CREATE TRIGGER trigger_assignment_override_poam_finding_lock
    BEFORE INSERT ON compliance_assignment_value_overrides
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_assignment();

CREATE OR REPLACE FUNCTION lock_poam_findings_for_bundle_state()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_bundle_id uuid;
    v_bundle_version_id uuid;
    v_key record;
BEGIN
    IF TG_TABLE_NAME='compliance_bundles' THEN
        v_bundle_id:=COALESCE((to_jsonb(NEW)->>'id')::uuid,(to_jsonb(OLD)->>'id')::uuid);
    ELSIF TG_TABLE_NAME='compliance_bundle_versions' THEN
        v_bundle_id:=COALESCE((to_jsonb(NEW)->>'bundle_id')::uuid,(to_jsonb(OLD)->>'bundle_id')::uuid);
        v_bundle_version_id:=COALESCE((to_jsonb(NEW)->>'id')::uuid,(to_jsonb(OLD)->>'id')::uuid);
    ELSE
        v_bundle_version_id:=COALESCE((to_jsonb(NEW)->>'bundle_version_id')::uuid,(to_jsonb(OLD)->>'bundle_version_id')::uuid);
        SELECT bundle_id INTO v_bundle_id FROM compliance_bundle_versions WHERE id=v_bundle_version_id;
    END IF;
    FOR v_key IN
        SELECT DISTINCT finding.system_id,finding.policy_lineage_id
        FROM compliance_bundle_assignments assignment
        JOIN systems system ON system.id=assignment.system_id OR system.environment_id=assignment.environment_id
        JOIN poam_findings finding ON finding.system_id=system.id
        JOIN compliance_bundle_assignment_versions version ON version.id=assignment.current_version_id
        WHERE assignment.bundle_id=v_bundle_id
           OR version.bundle_version_id=v_bundle_version_id
        ORDER BY finding.system_id,finding.policy_lineage_id
    LOOP
        PERFORM try_lock_poam_finding_key(v_key.system_id,v_key.policy_lineage_id);
    END LOOP;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_bundle_poam_finding_lock
    BEFORE UPDATE OR DELETE ON compliance_bundles
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_bundle_state();
CREATE TRIGGER trigger_bundle_version_poam_finding_lock
    BEFORE UPDATE OR DELETE ON compliance_bundle_versions
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_bundle_state();
CREATE TRIGGER trigger_bundle_membership_poam_finding_lock
    BEFORE INSERT OR UPDATE OR DELETE ON compliance_bundle_version_policies
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_bundle_state();

CREATE OR REPLACE FUNCTION lock_poam_findings_for_direct_applicability()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_system_id uuid;
    v_policy_id uuid;
    v_environment_id uuid;
    v_key record;
BEGIN
    v_policy_id := COALESCE((to_jsonb(NEW)->>'policy_id')::uuid, (to_jsonb(OLD)->>'policy_id')::uuid);
    v_system_id := COALESCE((to_jsonb(NEW)->>'system_id')::uuid, (to_jsonb(OLD)->>'system_id')::uuid);
    v_environment_id := COALESCE((to_jsonb(NEW)->>'environment_id')::uuid, (to_jsonb(OLD)->>'environment_id')::uuid);
    FOR v_key IN
        SELECT f.system_id, f.policy_lineage_id FROM poam_findings f
        JOIN systems s ON s.id = f.system_id
        WHERE f.policy_lineage_id IN (v_policy_id,(to_jsonb(OLD)->>'policy_id')::uuid)
          AND (f.system_id IN (v_system_id,(to_jsonb(OLD)->>'system_id')::uuid)
            OR s.environment_id IN (v_environment_id,(to_jsonb(OLD)->>'environment_id')::uuid))
        ORDER BY f.system_id, f.policy_lineage_id
    LOOP
        PERFORM try_lock_poam_finding_key(v_key.system_id, v_key.policy_lineage_id);
    END LOOP;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_system_policy_poam_finding_lock
    BEFORE INSERT OR UPDATE OR DELETE ON system_policies
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_direct_applicability();
CREATE TRIGGER trigger_environment_policy_poam_finding_lock
    BEFORE INSERT OR UPDATE OR DELETE ON environment_policies
    FOR EACH ROW EXECUTE FUNCTION lock_poam_findings_for_direct_applicability();

CREATE OR REPLACE FUNCTION lock_poam_finding_for_waiver()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
DECLARE
    v_finding_id uuid;
    v_key record;
BEGIN
    v_finding_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.finding_id ELSE NEW.finding_id END;
    SELECT system_id, policy_lineage_id INTO v_key FROM poam_findings WHERE id = v_finding_id;
    IF FOUND THEN
        PERFORM try_lock_poam_finding_key(v_key.system_id, v_key.policy_lineage_id);
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER trigger_finding_waiver_poam_finding_lock
    BEFORE INSERT OR UPDATE OR DELETE ON finding_waivers
    FOR EACH ROW EXECUTE FUNCTION lock_poam_finding_for_waiver();

CREATE OR REPLACE FUNCTION prevent_poam_history_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'POA&M activity and verification history is immutable';
END;
$$;

CREATE TRIGGER trigger_prevent_poam_activity_mutation
    BEFORE UPDATE OR DELETE ON poam_activity
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_history_mutation();
CREATE TRIGGER trigger_prevent_poam_verification_attempt_mutation
    BEFORE DELETE ON poam_verification_attempts
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_history_mutation();
CREATE OR REPLACE FUNCTION protect_poam_verification_attempt()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.sealed_at IS NULL AND NEW.sealed_at IS NOT NULL
       AND (to_jsonb(NEW)-'sealed_at') = (to_jsonb(OLD)-'sealed_at') THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'POA&M verification history is immutable';
END;
$$;
CREATE TRIGGER trigger_protect_poam_verification_attempt
    BEFORE UPDATE ON poam_verification_attempts
    FOR EACH ROW EXECUTE FUNCTION protect_poam_verification_attempt();
CREATE TRIGGER trigger_prevent_poam_verification_item_mutation
    BEFORE UPDATE OR DELETE ON poam_verification_items
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_history_mutation();
CREATE TRIGGER trigger_prevent_finding_waiver_event_mutation
    BEFORE UPDATE OR DELETE ON finding_waiver_events
    FOR EACH ROW EXECUTE FUNCTION prevent_poam_history_mutation();

CREATE OR REPLACE FUNCTION protect_finding_waiver_evidence()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='DELETE' THEN
        RAISE EXCEPTION 'Finding waiver evidence is immutable';
    END IF;
    IF NEW.finding_id<>OLD.finding_id OR NEW.justification<>OLD.justification
       OR NEW.policy_version_id<>OLD.policy_version_id OR NEW.assessment_id<>OLD.assessment_id
       OR NEW.observation_token<>OLD.observation_token
       OR NEW.observation_snapshot IS DISTINCT FROM OLD.observation_snapshot
       OR NEW.created_by<>OLD.created_by
       OR NEW.created_at<>OLD.created_at
       OR (OLD.accepted_by IS NOT NULL AND (NEW.accepted_by IS DISTINCT FROM OLD.accepted_by
           OR NEW.accepted_at IS DISTINCT FROM OLD.accepted_at)) THEN
        RAISE EXCEPTION 'Finding waiver identity and acceptance evidence are immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_protect_finding_waiver_evidence
    BEFORE UPDATE OR DELETE ON finding_waivers
    FOR EACH ROW EXECUTE FUNCTION protect_finding_waiver_evidence();

CREATE OR REPLACE FUNCTION enforce_finding_waiver_lifecycle()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='INSERT' THEN
        IF NEW.status<>'pending' OR NEW.accepted_by IS NOT NULL
           OR NEW.accepted_at IS NOT NULL OR NEW.expires_at IS NOT NULL THEN
            RAISE EXCEPTION 'A finding waiver must begin pending'
                USING ERRCODE='23514', CONSTRAINT='finding_waiver_initial_state';
        END IF;
        RETURN NEW;
    END IF;
    IF NEW.status IS DISTINCT FROM OLD.status AND NOT (
        (OLD.status='pending' AND NEW.status IN ('accepted','rejected'))
        OR (OLD.status='accepted' AND NEW.status IN ('revoked','expired'))
    ) THEN
        RAISE EXCEPTION 'Invalid finding waiver transition'
            USING ERRCODE='23514', CONSTRAINT='finding_waiver_transition';
    END IF;
    IF NEW.expires_at IS DISTINCT FROM OLD.expires_at
       AND NOT (OLD.status='pending' AND NEW.status='accepted') THEN
        RAISE EXCEPTION 'Finding waiver expiry is immutable after acceptance'
            USING ERRCODE='23514', CONSTRAINT='finding_waiver_expiry_immutable';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER trigger_enforce_finding_waiver_lifecycle
    BEFORE INSERT OR UPDATE ON finding_waivers
    FOR EACH ROW EXECUTE FUNCTION enforce_finding_waiver_lifecycle();
