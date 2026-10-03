-- CONCURRENCY: Migrations normally run before server startup, but an older
-- server or a direct SQL writer may still insert source rows during upgrade.
-- SHARE ROW EXCLUSIVE conflicts with INSERT's ROW EXCLUSIVE lock. Acquire all
-- three source-table locks before reading either sources or identity mappings.
-- Hold them through repair and trigger validation in this migration transaction.
LOCK TABLE finding_waivers, cve_system_dispositions, cve_environment_dispositions
    IN SHARE ROW EXCLUSIVE MODE;
-- CVE renewal audit is the sole authoritative CVE predecessor link. Freeze
-- direct audit writers after the source locks so the same lineage is used for
-- validation and repair. Normal renewal writes sources before audit.
LOCK TABLE admin_audit_events IN SHARE MODE;

DO $$
DECLARE
    largest_number bigint;
    sequence_number bigint;
BEGIN
    CREATE TEMP TABLE ra_0298_nodes ON COMMIT DROP AS
      SELECT 'policy_waiver'::text AS kind,id FROM finding_waivers
      UNION ALL SELECT 'cve_host',id FROM cve_system_dispositions
      UNION ALL SELECT 'cve_environment',id FROM cve_environment_dispositions;
    CREATE TEMP TABLE ra_0298_edges ON COMMIT DROP AS
      SELECT 'policy_waiver'::text AS kind,id,predecessor_id AS parent
        FROM finding_waivers WHERE predecessor_id IS NOT NULL;

    IF EXISTS (SELECT 1 FROM admin_audit_events
               WHERE action='cve_acceptance_renewed' AND
                 (COALESCE(metadata->>'source_type','') NOT IN ('host','environment')
                  OR COALESCE(metadata->>'predecessor_id','') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                   OR COALESCE(metadata->>'successor_id','') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$')) THEN
        RAISE EXCEPTION 'Invalid historical CVE renewal lineage';
    END IF;
    INSERT INTO ra_0298_edges(kind,id,parent)
      SELECT CASE metadata->>'source_type' WHEN 'host' THEN 'cve_host' ELSE 'cve_environment' END,
             (metadata->>'successor_id')::uuid,(metadata->>'predecessor_id')::uuid
      FROM admin_audit_events WHERE action='cve_acceptance_renewed';
    IF EXISTS (SELECT 1 FROM ra_0298_edges edge
               LEFT JOIN ra_0298_nodes child ON child.kind=edge.kind AND child.id=edge.id
               LEFT JOIN ra_0298_nodes parent ON parent.kind=edge.kind AND parent.id=edge.parent
               WHERE child.id IS NULL OR parent.id IS NULL OR edge.id=edge.parent)
       OR EXISTS (SELECT 1 FROM ra_0298_edges GROUP BY kind,id HAVING count(*)<>1)
       OR EXISTS (SELECT 1 FROM ra_0298_edges GROUP BY kind,parent HAVING count(*)>1) THEN
        RAISE EXCEPTION 'Ambiguous or missing historical acceptance renewal source';
    END IF;
    IF EXISTS (
      SELECT 1 FROM (
        SELECT 'cve_host'::text AS kind,id FROM cve_system_dispositions WHERE retirement_reason='renewed'
        UNION ALL SELECT 'cve_environment',id FROM cve_environment_dispositions WHERE retirement_reason='renewed'
      ) retired LEFT JOIN ra_0298_edges edge ON edge.kind=retired.kind AND edge.parent=retired.id
      WHERE edge.id IS NULL
    ) THEN RAISE EXCEPTION 'Missing authoritative historical CVE renewal lineage'; END IF;
    IF EXISTS (
      SELECT 1 FROM ra_0298_edges edge
      LEFT JOIN cve_system_dispositions host_parent ON edge.kind='cve_host' AND host_parent.id=edge.parent
      LEFT JOIN cve_system_dispositions host_child ON edge.kind='cve_host' AND host_child.id=edge.id
      LEFT JOIN cve_environment_dispositions env_parent ON edge.kind='cve_environment' AND env_parent.id=edge.parent
      LEFT JOIN cve_environment_dispositions env_child ON edge.kind='cve_environment' AND env_child.id=edge.id
      WHERE (edge.kind='cve_host' AND
             (host_parent.retirement_reason IS DISTINCT FROM 'renewed'
              OR host_parent.retired_at IS NULL OR host_child.state IS DISTINCT FROM 'accepted'
              OR host_parent.system_id IS DISTINCT FROM host_child.system_id
              OR host_parent.canonical_cve_id IS DISTINCT FROM host_child.canonical_cve_id
              OR host_parent.canonical_package_name IS DISTINCT FROM host_child.canonical_package_name))
         OR (edge.kind='cve_environment' AND
             (env_parent.retirement_reason IS DISTINCT FROM 'renewed'
              OR env_parent.retired_at IS NULL OR env_child.state IS DISTINCT FROM 'accepted'
              OR env_parent.environment_id IS DISTINCT FROM env_child.environment_id
              OR env_parent.canonical_cve_id IS DISTINCT FROM env_child.canonical_cve_id
              OR env_parent.canonical_package_name IS DISTINCT FROM env_child.canonical_package_name))
    ) THEN RAISE EXCEPTION 'Inconsistent historical CVE renewal lineage'; END IF;

    -- Each source follows only recorded predecessors. Mapping a successor from
    -- a matching CVE/scope tuple without an audit edge would invent authority.
    CREATE TEMP TABLE ra_0298_roots ON COMMIT DROP AS
      WITH RECURSIVE ancestors AS (
        SELECT kind,id,id AS ancestor,ARRAY[id] AS path,false AS cycle FROM ra_0298_nodes
        UNION ALL
        SELECT walk.kind,walk.id,edge.parent,walk.path || edge.parent,
               edge.parent=ANY(walk.path)
        FROM ancestors walk JOIN ra_0298_edges edge
          ON edge.kind=walk.kind AND edge.id=walk.ancestor
        WHERE NOT walk.cycle
      )
      SELECT walk.kind,walk.id,walk.ancestor AS root,walk.cycle
      FROM ancestors walk LEFT JOIN ra_0298_edges edge
        ON edge.kind=walk.kind AND edge.id=walk.ancestor
      WHERE edge.id IS NULL OR walk.cycle;
    IF EXISTS (SELECT 1 FROM ra_0298_roots WHERE cycle)
       OR EXISTS (SELECT 1 FROM ra_0298_roots GROUP BY kind,id HAVING count(*)<>1)
       OR EXISTS (SELECT 1 FROM risk_acceptance_source_ids identity
                  LEFT JOIN ra_0298_nodes source
                    ON source.kind=identity.source_kind AND source.id=identity.source_id
                  WHERE source.id IS NULL) THEN
        RAISE EXCEPTION 'Invalid existing risk acceptance identity or renewal chain';
    END IF;
    CREATE TEMP TABLE ra_0298_chains ON COMMIT DROP AS
      SELECT root.kind,root.root,max(identity.human_number) AS human_number
      FROM ra_0298_roots root
      LEFT JOIN risk_acceptance_source_ids identity
        ON identity.source_kind=root.kind AND identity.source_id=root.id
      GROUP BY root.kind,root.root;
    IF EXISTS (
      SELECT 1 FROM ra_0298_roots root JOIN risk_acceptance_source_ids identity
        ON identity.source_kind=root.kind AND identity.source_id=root.id
      GROUP BY root.kind,root.root HAVING count(DISTINCT identity.human_number)>1
    ) OR EXISTS (
      SELECT 1 FROM ra_0298_chains WHERE human_number IS NOT NULL
      GROUP BY human_number HAVING count(*)>1
    ) THEN RAISE EXCEPTION 'Conflicting existing risk acceptance chain numbers'; END IF;

    -- Existing numbers are immutable. A sequence number is consumed only for
    -- a wholly unmapped root, never for a missing successor of a mapped chain.
    SELECT COALESCE(max(human_number),0) INTO largest_number FROM risk_acceptance_source_ids;
    SELECT last_value INTO sequence_number FROM risk_acceptance_number_seq;
    IF largest_number>sequence_number THEN
        PERFORM setval('risk_acceptance_number_seq',largest_number,true);
    END IF;
    UPDATE ra_0298_chains SET human_number=nextval('risk_acceptance_number_seq')
      WHERE human_number IS NULL;
    INSERT INTO risk_acceptance_source_ids(source_kind,source_id,human_number)
      SELECT root.kind,root.id,chain.human_number
      FROM ra_0298_roots root JOIN ra_0298_chains chain
        ON chain.kind=root.kind AND chain.root=root.root
      LEFT JOIN risk_acceptance_source_ids identity
        ON identity.source_kind=root.kind AND identity.source_id=root.id
      WHERE identity.source_id IS NULL;

    IF EXISTS (
      SELECT 1 FROM ra_0298_nodes source LEFT JOIN risk_acceptance_source_ids identity
        ON identity.source_kind=source.kind AND identity.source_id=source.id
      WHERE identity.source_id IS NULL
    ) OR EXISTS (
      SELECT 1 FROM ra_0298_roots root JOIN risk_acceptance_source_ids identity
        ON identity.source_kind=root.kind AND identity.source_id=root.id
      JOIN ra_0298_chains chain ON chain.kind=root.kind AND chain.root=root.root
      WHERE identity.human_number<>chain.human_number
    ) OR EXISTS (
      SELECT 1 FROM ra_0298_chains GROUP BY human_number HAVING count(*)>1
    ) THEN RAISE EXCEPTION 'Risk acceptance repair did not preserve unique chains'; END IF;

    -- A missing or disabled source trigger would reopen the first-visibility
    -- gap immediately after releasing the table locks. Reject that upgrade.
    IF (SELECT count(*) FROM (
      VALUES ('finding_waivers'::regclass),('cve_system_dispositions'::regclass),
             ('cve_environment_dispositions'::regclass)
    ) source(table_id) JOIN pg_trigger trigger ON trigger.tgrelid=source.table_id
      WHERE trigger.tgname=CASE source.table_id
        WHEN 'finding_waivers'::regclass THEN 'risk_acceptance_waiver_insert'
        WHEN 'cve_system_dispositions'::regclass THEN 'risk_acceptance_host_insert'
        ELSE 'risk_acceptance_environment_insert' END
        AND trigger.tgfoid='assign_risk_acceptance_source_id()'::regprocedure
        AND trigger.tgenabled IN ('O','A')
        AND (trigger.tgtype & 1)=1 AND (trigger.tgtype & 4)=4
        AND (trigger.tgtype & 2)=0)<>3 THEN
        RAISE EXCEPTION 'Risk acceptance source insert trigger coverage is incomplete';
    END IF;
END $$;
