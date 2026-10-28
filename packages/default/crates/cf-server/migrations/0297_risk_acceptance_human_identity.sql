-- RA numbers identify an operator-facing renewal chain, not a source row or
-- a new decision authority. Source kind and UUID remain the mutation version.
CREATE SEQUENCE risk_acceptance_number_seq AS bigint;
CREATE TABLE risk_acceptance_source_ids (
    source_kind text NOT NULL CHECK (source_kind IN ('policy_waiver','cve_host','cve_environment')),
    source_id uuid NOT NULL,
    human_number bigint NOT NULL CHECK (human_number > 0),
    PRIMARY KEY (source_kind,source_id)
);
CREATE INDEX risk_acceptance_source_ids_number_idx ON risk_acceptance_source_ids(human_number);

-- COMPATIBILITY: Only the waiver predecessor FK and the committed CVE renewal
-- audit establish ancestry. A matching CVE/scope tuple does not establish it.
-- Reject incomplete/ambiguous historical chains instead of assigning a false
-- new RA number to a successor.
DO $$
DECLARE
    missing_count bigint;
    row_count bigint;
BEGIN
    CREATE TEMP TABLE ra_nodes ON COMMIT DROP AS
      SELECT 'policy_waiver'::text AS kind,id,predecessor_id AS parent FROM finding_waivers
      UNION ALL SELECT 'cve_host',id,NULL::uuid FROM cve_system_dispositions
      UNION ALL SELECT 'cve_environment',id,NULL::uuid FROM cve_environment_dispositions;
    CREATE TEMP TABLE ra_edges ON COMMIT DROP AS
      SELECT kind,id,parent FROM ra_nodes WHERE parent IS NOT NULL;

    IF EXISTS (SELECT 1 FROM admin_audit_events
               WHERE action='cve_acceptance_renewed' AND
                 (COALESCE(metadata->>'source_type','') NOT IN ('host','environment')
                  OR COALESCE(metadata->>'predecessor_id','') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                  OR COALESCE(metadata->>'successor_id','') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')) THEN
        RAISE EXCEPTION 'Invalid historical CVE renewal lineage';
    END IF;
    INSERT INTO ra_edges(kind,id,parent)
      SELECT CASE metadata->>'source_type' WHEN 'host' THEN 'cve_host' ELSE 'cve_environment' END,
             (metadata->>'successor_id')::uuid,(metadata->>'predecessor_id')::uuid
      FROM admin_audit_events WHERE action='cve_acceptance_renewed';
    IF EXISTS (SELECT 1 FROM ra_edges e LEFT JOIN ra_nodes child ON child.kind=e.kind AND child.id=e.id
               LEFT JOIN ra_nodes parent ON parent.kind=e.kind AND parent.id=e.parent
               WHERE child.id IS NULL OR parent.id IS NULL OR e.id=e.parent)
       OR EXISTS (SELECT 1 FROM ra_edges GROUP BY kind,id HAVING count(*)<>1)
       OR EXISTS (SELECT 1 FROM ra_edges GROUP BY kind,parent HAVING count(*)>1) THEN
        RAISE EXCEPTION 'Ambiguous or missing historical acceptance renewal source';
    END IF;
    SELECT count(*) INTO missing_count FROM (
      SELECT 'cve_host'::text AS kind,id FROM cve_system_dispositions
        WHERE retirement_reason='renewed'
      UNION ALL SELECT 'cve_environment',id FROM cve_environment_dispositions
        WHERE retirement_reason='renewed'
    ) retired LEFT JOIN ra_edges edge ON edge.kind=retired.kind AND edge.parent=retired.id
    WHERE edge.id IS NULL;
    IF missing_count<>0 THEN
        RAISE EXCEPTION 'Missing authoritative historical CVE renewal lineage for % rows',missing_count;
    END IF;
    IF EXISTS (
      SELECT 1 FROM ra_edges edge
      LEFT JOIN cve_system_dispositions host_parent ON edge.kind='cve_host' AND host_parent.id=edge.parent
      LEFT JOIN cve_system_dispositions host_child ON edge.kind='cve_host' AND host_child.id=edge.id
      LEFT JOIN cve_environment_dispositions env_parent ON edge.kind='cve_environment' AND env_parent.id=edge.parent
      LEFT JOIN cve_environment_dispositions env_child ON edge.kind='cve_environment' AND env_child.id=edge.id
      WHERE (edge.kind='cve_host' AND
             (host_parent.retirement_reason IS DISTINCT FROM 'renewed'
              OR host_parent.system_id IS DISTINCT FROM host_child.system_id
              OR host_parent.canonical_cve_id IS DISTINCT FROM host_child.canonical_cve_id
              OR host_parent.canonical_package_name IS DISTINCT FROM host_child.canonical_package_name))
         OR (edge.kind='cve_environment' AND
             (env_parent.retirement_reason IS DISTINCT FROM 'renewed'
              OR env_parent.environment_id IS DISTINCT FROM env_child.environment_id
              OR env_parent.canonical_cve_id IS DISTINCT FROM env_child.canonical_cve_id
              OR env_parent.canonical_package_name IS DISTINCT FROM env_child.canonical_package_name))
    ) THEN RAISE EXCEPTION 'Inconsistent historical CVE renewal lineage'; END IF;

    -- Each node walks only its recorded predecessor chain. Detect cycles and
    -- give every distinct root exactly one globally numbered identity.
    CREATE TEMP TABLE ra_roots ON COMMIT DROP AS
      WITH RECURSIVE ancestors AS (
        SELECT kind,id,id AS ancestor,ARRAY[id] AS path,false AS cycle FROM ra_nodes
        UNION ALL
        SELECT a.kind,a.id,e.parent,a.path || e.parent,e.parent=ANY(a.path)
        FROM ancestors a JOIN ra_edges e ON e.kind=a.kind AND e.id=a.ancestor
        WHERE NOT a.cycle
      )
      SELECT a.kind,a.id,a.ancestor AS root,a.cycle
      FROM ancestors a LEFT JOIN ra_edges e ON e.kind=a.kind AND e.id=a.ancestor
      WHERE e.id IS NULL OR a.cycle;
    IF EXISTS (SELECT 1 FROM ra_roots WHERE cycle)
       OR EXISTS (SELECT 1 FROM ra_roots GROUP BY kind,id HAVING count(*)<>1) THEN
        RAISE EXCEPTION 'Cyclic or ambiguous historical acceptance renewal lineage';
    END IF;
    INSERT INTO risk_acceptance_source_ids(source_kind,source_id,human_number)
      SELECT kind,id,dense_rank() OVER (ORDER BY kind,root) FROM ra_roots;
    SELECT count(DISTINCT (kind,root)) INTO row_count FROM ra_roots;
    IF row_count>0 THEN
        PERFORM setval('risk_acceptance_number_seq',row_count,true);
    END IF;
END $$;

-- Source insert triggers cover direct SQL writers and guarantee that a newly
-- inserted decision has a visible identity in its insertion transaction.
CREATE FUNCTION assign_risk_acceptance_source_id() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE kind text;
BEGIN
    kind := CASE TG_TABLE_NAME WHEN 'finding_waivers' THEN 'policy_waiver'
             WHEN 'cve_system_dispositions' THEN 'cve_host'
             WHEN 'cve_environment_dispositions' THEN 'cve_environment' END;
    INSERT INTO risk_acceptance_source_ids(source_kind,source_id,human_number)
      VALUES(kind,NEW.id,nextval('risk_acceptance_number_seq'));
    -- The waiver predecessor FK is authoritative even for direct SQL writers.
    IF kind='policy_waiver' AND to_jsonb(NEW)->>'predecessor_id' IS NOT NULL THEN
      UPDATE risk_acceptance_source_ids child SET human_number=parent.human_number
      FROM risk_acceptance_source_ids parent
      WHERE child.source_kind=kind AND child.source_id=NEW.id
        AND parent.source_kind=kind AND parent.source_id=(to_jsonb(NEW)->>'predecessor_id')::uuid;
      IF NOT FOUND THEN RAISE EXCEPTION 'Missing waiver predecessor RA identity'; END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER risk_acceptance_waiver_insert AFTER INSERT ON finding_waivers
  FOR EACH ROW EXECUTE FUNCTION assign_risk_acceptance_source_id();
CREATE TRIGGER risk_acceptance_host_insert AFTER INSERT ON cve_system_dispositions
  FOR EACH ROW EXECUTE FUNCTION assign_risk_acceptance_source_id();
CREATE TRIGGER risk_acceptance_environment_insert AFTER INSERT ON cve_environment_dispositions
  FOR EACH ROW EXECUTE FUNCTION assign_risk_acceptance_source_id();

-- CONCURRENCY: The owning CVE renewal transaction locks the predecessor and
-- active decision before inserting the successor. Rebind only that new row in
-- the same transaction; a retry must never allocate a second visible number.
CREATE FUNCTION link_cve_risk_acceptance_id(kind text, predecessor uuid, successor uuid)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE
    scope_kind text;
    source_table text;
    scope_column text;
    lineage_valid boolean;
BEGIN
    IF kind NOT IN ('cve_host','cve_environment') OR predecessor=successor THEN
        RAISE EXCEPTION 'Invalid CVE acceptance lineage';
    END IF;
    scope_kind := CASE kind WHEN 'cve_host' THEN 'host' ELSE 'environment' END;
    source_table := CASE kind WHEN 'cve_host' THEN 'cve_system_dispositions'
                   ELSE 'cve_environment_dispositions' END;
    scope_column := CASE kind WHEN 'cve_host' THEN 'system_id' ELSE 'environment_id' END;
    -- SECURITY: The mapping is only an identity projection. A caller cannot
    -- invent a chain by naming two rows with the same CVE/scope tuple.
    EXECUTE format('SELECT EXISTS (SELECT 1 FROM %I parent JOIN %I child
      ON child.canonical_cve_id=parent.canonical_cve_id
      AND child.canonical_package_name=parent.canonical_package_name
      AND child.%I=parent.%I WHERE parent.id=$1 AND child.id=$2
      AND parent.retirement_reason=''renewed'' AND parent.retired_at IS NOT NULL
      AND child.state=''accepted'')',source_table,source_table,scope_column,scope_column)
      INTO lineage_valid USING predecessor,successor;
    IF NOT lineage_valid OR (
        SELECT count(*) FROM admin_audit_events event
        WHERE event.action='cve_acceptance_renewed'
          AND event.metadata->>'source_type'=scope_kind
          AND event.metadata->>'predecessor_id'=predecessor::text
          AND event.metadata->>'successor_id'=successor::text
    )<>1 OR EXISTS (
        SELECT 1 FROM admin_audit_events event
        WHERE event.action='cve_acceptance_renewed'
          AND event.metadata->>'source_type'=scope_kind
          AND (event.metadata->>'predecessor_id'=predecessor::text
            OR event.metadata->>'successor_id'=successor::text)
          AND (event.metadata->>'predecessor_id'<>predecessor::text
            OR event.metadata->>'successor_id'<>successor::text)
    ) THEN RAISE EXCEPTION 'Missing or ambiguous audited CVE renewal lineage'; END IF;
    UPDATE risk_acceptance_source_ids child SET human_number=parent.human_number
      FROM risk_acceptance_source_ids parent
      WHERE child.source_kind=kind AND child.source_id=successor
        AND parent.source_kind=kind AND parent.source_id=predecessor;
    IF NOT FOUND THEN RAISE EXCEPTION 'Missing CVE acceptance lineage mapping'; END IF;
END $$;
