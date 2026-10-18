//! Exact-attempt discovery and current-membership projection for summaries.

/// Authorizes a represented attempt, not every historical job on a shared flake.
/// Before an active attempt captures its plan, current active registration grants
/// only coarse queue visibility. Terminal history requires captured UUID proof.
pub(super) fn visibility(user_bind: &str, allow_pending: bool) -> String {
    format!(
        r#"(
        {user_bind}::uuid IS NULL OR EXISTS (
            SELECT 1 FROM evaluation_policy_plans p,
                jsonb_array_elements(p.payload->'rows') captured,
                systems s JOIN user_environment_memberships m ON m.environment_id=s.environment_id
            WHERE p.commit_id=c.id AND p.evaluation_attempt_id=ea.id
                AND p.attempt_number=ea.attempt_number
                AND p.payload->>'provenance'='captured'
                AND captured->>'system_id'=s.id::text AND s.flake_id=c.flake_id
                AND captured->>'configuration_name'=COALESCE(NULLIF(btrim(s.system_configuration_name),''),s.hostname)
                AND m.user_id={user_bind}
        ) OR ({allow_pending} AND c.evaluation_status IN ('pending','in_progress','cancelling')
            AND NOT EXISTS (SELECT 1 FROM evaluation_policy_plans p
                WHERE p.evaluation_attempt_id=ea.id AND p.commit_id=c.id
                    AND p.attempt_number=ea.attempt_number)
            AND EXISTS (SELECT 1 FROM systems s JOIN user_environment_memberships m
                ON m.environment_id=s.environment_id WHERE s.flake_id=c.flake_id
                    AND s.is_active AND m.user_id={user_bind}))
    )"#
    )
}

/// Constructs joins for commit alias `c` and a trusted SQL user placeholder.
///
/// `None` at the bound UUID denotes the internal administrator projection.
/// Names require a retained captured UUID/configuration with current access.
/// After discovery the primary population is planned union discovered names;
/// a planned name absent from discovery stays unavailable, not executed.
/// Configuration-wide policy
/// evidence requires every current host to be visible because the native gate
/// can contain policy facts from any host sharing that configuration. Discovery
/// and selection remain separate: unselected names never imply pending work.
pub(super) fn join(user_bind: &str) -> String {
    format!(
        r#"
        LEFT JOIN LATERAL (
            SELECT id, attempt_number, parent_attempt_id, root_attempt_id, available_at, status
            FROM evaluation_attempts WHERE commit_id = c.id
            ORDER BY attempt_number DESC, created_at DESC LIMIT 1
        ) ea ON TRUE
        LEFT JOIN LATERAL (
            WITH inventory AS (
                SELECT i.* FROM evaluation_attempt_inventory i
                WHERE i.commit_id = c.id AND i.evaluation_attempt_id = ea.id
                  AND i.attempt_number = ea.attempt_number
             ), planned AS (
                 SELECT captured->>'configuration_name' AS name,
                     captured->>'system_id' AS system_id
                 FROM evaluation_policy_plans p,
                     jsonb_array_elements(p.payload->'rows') captured
                 WHERE p.commit_id=c.id AND p.evaluation_attempt_id=ea.id
                     AND p.attempt_number=ea.attempt_number
             ), names AS (
                 SELECT DISTINCT name FROM inventory i,
                     unnest(i.configuration_names) AS name
                 UNION SELECT name FROM planned
            ), hosts AS (
                SELECT s.id, COALESCE(NULLIF(btrim(s.system_configuration_name), ''), s.hostname) AS name,
                    EXISTS (SELECT 1 FROM user_environment_memberships uem
                        WHERE uem.environment_id = s.environment_id
                          AND uem.user_id = {user_bind}) AS visible
                FROM systems s WHERE s.flake_id = c.flake_id
            ), current_scope AS (
                SELECT name,bool_and(visible) AS all_visible FROM hosts GROUP BY name
            ), captured_scope AS (
                SELECT p.name,bool_or(COALESCE(h.visible,false)) AS any_visible,
                    bool_and(COALESCE(h.visible,false)) AS all_visible
                FROM planned p LEFT JOIN hosts h ON h.id::text=p.system_id AND h.name=p.name
                GROUP BY p.name
            ), visible AS (
                 SELECT n.name FROM names n LEFT JOIN captured_scope s ON s.name=n.name
                 WHERE EXISTS (SELECT 1 FROM inventory)
                     AND ({user_bind}::uuid IS NULL OR COALESCE(s.any_visible,false))
            )
            SELECT EXISTS (SELECT 1 FROM inventory) AS inventory_available,
                (NOT COALESCE(c.cancellation_requested,false) AND
                    (ea.status='queued' OR (ea.status='in_progress'
                        AND c.evaluation_attempt_count=ea.attempt_number
                        AND c.evaluation_status='in_progress'))) AS can_complete,
                ARRAY(SELECT name FROM visible ORDER BY name) AS systems,
                ARRAY(SELECT name FROM visible v WHERE EXISTS (SELECT 1 FROM inventory i
                    WHERE NOT v.name=ANY(i.configuration_names))
                    ORDER BY name) AS missing_configurations,
                ARRAY(SELECT name FROM visible v WHERE EXISTS (
                    SELECT 1 FROM inventory i WHERE v.name=ANY(i.configuration_names)
                        AND NOT v.name=ANY(i.selected_configuration_names))
                    ORDER BY name) AS excluded_configurations,
                ARRAY(SELECT name FROM visible v
                    WHERE EXISTS (SELECT 1 FROM inventory i
                        WHERE v.name = ANY(i.selected_configuration_names)
                            OR NOT v.name = ANY(i.configuration_names))
                    ORDER BY name) AS policy_configurations,
                ARRAY(SELECT name FROM visible v WHERE EXISTS (SELECT 1 FROM inventory i
                    WHERE v.name=ANY(i.selected_configuration_names))
                    ORDER BY name) AS agent_configurations,
                ARRAY(SELECT v.name FROM visible v
                    LEFT JOIN current_scope cscope ON cscope.name=v.name
                    LEFT JOIN captured_scope pscope ON pscope.name=v.name
                    WHERE EXISTS (SELECT 1 FROM inventory i
                        WHERE v.name = ANY(i.selected_configuration_names))
                      AND ({user_bind}::uuid IS NULL OR (
                        COALESCE(cscope.all_visible,false) AND COALESCE(pscope.all_visible,false)))
                    ORDER BY v.name) AS evidence_configurations
        ) inv ON TRUE
        "#
    )
}
