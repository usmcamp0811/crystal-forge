-- Durable POA&M notification sources and authorization.

ALTER TABLE attention_occurrences
    DROP CONSTRAINT attention_occurrences_category_check;
ALTER TABLE attention_occurrences
    ADD CONSTRAINT attention_occurrences_category_check CHECK (
        category IN ('builds', 'evals', 'flakes', 'systems', 'environments', 'cves', 'poams')
    );

-- The advisory lock serializes normal writers. This index is the database
-- backstop that prevents concurrent or future writers from opening two overdue
-- episodes for the same POA&M.
CREATE UNIQUE INDEX attention_occurrences_one_open_poam_overdue
    ON attention_occurrences (subject_id)
    WHERE category = 'poams'
      AND resolved_at IS NULL
      AND metadata->>'reason' = 'overdue';

CREATE INDEX poam_activity_awaiting_verification_notifications
    ON poam_activity (created_at, id)
    WHERE kind = 'status_changed'
      AND payload->>'to' = 'awaiting_verification';

CREATE INDEX deployment_policy_versions_setup_coach_created_by
    ON deployment_policy_versions (policy_id)
    WHERE created_by IS NOT NULL;

CREATE OR REPLACE FUNCTION notification_visible_to_user(
    p_user_id UUID,
    p_source_type TEXT,
    p_source_id TEXT
) RETURNS BOOLEAN
LANGUAGE SQL
STABLE
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM users u
        JOIN user_role_assignments ura ON ura.user_id = u.id
        WHERE u.id = p_user_id
          AND u.is_active = TRUE
          AND p_source_type IN ('builds', 'evals', 'cves')
    )
    OR (
        p_source_type = 'systems'
        AND EXISTS (
            SELECT 1
            FROM users u
            JOIN user_role_assignments ura ON ura.user_id = u.id
            JOIN systems s ON s.id::text = p_source_id
            LEFT JOIN user_environment_memberships uem
              ON uem.environment_id = s.environment_id
             AND uem.user_id = p_user_id
            WHERE u.id = p_user_id
              AND u.is_active = TRUE
              AND (ura.role = 'admin' OR uem.user_id IS NOT NULL)
        )
    )
    OR (
        p_source_type = 'system_event'
        AND EXISTS (
            SELECT 1
            FROM users u
            JOIN user_role_assignments ura ON ura.user_id = u.id
            JOIN system_events se ON se.id::text = p_source_id
            JOIN systems s ON s.id = se.system_id
            LEFT JOIN user_environment_memberships uem
              ON uem.environment_id = s.environment_id
             AND uem.user_id = p_user_id
            WHERE u.id = p_user_id
              AND u.is_active = TRUE
              AND (ura.role = 'admin' OR uem.user_id IS NOT NULL)
        )
    )
    OR (
        p_source_type = 'poams'
        AND EXISTS (
            SELECT 1
            FROM users u
            JOIN user_role_assignments ura ON ura.user_id = u.id
            JOIN poams poam ON poam.id::text = p_source_id
            WHERE u.id = p_user_id
              AND u.is_active = TRUE
              AND (
                  ura.role = 'admin'
                  OR poam_visible_to_environments(
                      poam.id,
                      ARRAY(
                          SELECT uem.environment_id
                          FROM user_environment_memberships uem
                          WHERE uem.user_id = p_user_id
                      )
                  )
              )
        )
    );
$$;
