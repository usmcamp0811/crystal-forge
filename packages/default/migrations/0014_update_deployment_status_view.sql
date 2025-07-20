CREATE OR REPLACE VIEW view_systems_status_table AS
WITH system_derivation_source AS (
    -- Find if current derivation path maps to any known evaluation target
    SELECT 
        sys.hostname,
        sys.current_derivation_path,
        sys.latest_commit_derivation_path,
        sys.latest_commit_evaluation_status,
        sys.last_seen,
        sys.agent_version,
        sys.uptime_days,
        sys.ip_address,
        sys.last_deployed,
        -- Check if current derivation matches any completed evaluation
        CASE WHEN EXISTS (
            SELECT 1 FROM evaluation_targets et 
            WHERE et.derivation_path = sys.current_derivation_path 
            AND et.status = 'complete'
        ) THEN TRUE ELSE FALSE END AS derivation_is_known,
        
        -- Get the most recent commit timestamp for this system's flake
        latest_commit.commit_timestamp AS latest_flake_commit_timestamp,
        
        -- Check if there are pending/in-progress evaluations for newer commits
        CASE WHEN EXISTS (
            SELECT 1 FROM evaluation_targets et
            JOIN commits c ON et.commit_id = c.id
            JOIN flakes f ON c.flake_id = f.id
            JOIN systems s ON s.hostname = sys.hostname
            WHERE s.flake_id = f.id
            AND et.target_name = sys.hostname
            AND et.status IN ('pending', 'queued', 'in-progress')
            AND c.commit_timestamp > sys.last_deployed
        ) THEN TRUE ELSE FALSE END AS has_pending_newer_evaluation,
        
        -- Get timestamp of oldest pending evaluation (to check for stuck evaluations)
        (SELECT MIN(et.scheduled_at) 
         FROM evaluation_targets et
         JOIN commits c ON et.commit_id = c.id
         JOIN flakes f ON c.flake_id = f.id
         JOIN systems s ON s.hostname = sys.hostname
         WHERE s.flake_id = f.id
         AND et.target_name = sys.hostname
         AND et.status IN ('pending', 'queued', 'in-progress')
        ) AS oldest_pending_eval_time
        
    FROM view_systems_current_state sys
    LEFT JOIN (
        -- Get latest commit timestamp for each system's flake
        SELECT DISTINCT ON (s.hostname)
            s.hostname,
            c.commit_timestamp
        FROM systems s
        JOIN flakes f ON s.flake_id = f.id
        JOIN commits c ON f.id = c.flake_id
        ORDER BY s.hostname, c.commit_timestamp DESC
    ) latest_commit ON sys.hostname = latest_commit.hostname
)
SELECT
    hostname,
    -- Determine status symbol based on the new logic
    CASE 
        -- No system state at all
        WHEN current_derivation_path IS NULL THEN '⚫' -- No System State
        
        -- Current derivation is known (maps to completed evaluation)
        WHEN derivation_is_known = TRUE THEN
            CASE 
                -- Running latest evaluated derivation
                WHEN current_derivation_path = latest_commit_derivation_path THEN '🟢' -- Current
                
                -- Running older known derivation, but evaluation pending for newer commit
                WHEN has_pending_newer_evaluation = TRUE THEN
                    CASE 
                        -- Evaluation has been pending too long (>1 hour), mark as unknown
                        WHEN oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN '🟤' -- Unknown (stuck evaluation)
                        -- Normal evaluation in progress
                        ELSE '🟡' -- Evaluating
                    END
                
                -- Running older known derivation, no newer commits to evaluate
                ELSE '🔴' -- Behind
            END
        
        -- Current derivation is unknown (doesn't map to any completed evaluation)
        WHEN derivation_is_known = FALSE THEN
            CASE 
                -- There was a new commit since last deployment, might be evaluating
                WHEN latest_flake_commit_timestamp > last_deployed AND has_pending_newer_evaluation = TRUE THEN
                    CASE 
                        -- Evaluation has been pending too long (>1 hour)
                        WHEN oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN '🟤' -- Unknown (stuck evaluation)
                        -- Normal evaluation in progress
                        ELSE '🟡' -- Evaluating
                    END
                
                -- No new commits or no pending evaluations - truly unknown
                ELSE '🟤' -- Unknown
            END
        
        -- Fallback
        ELSE '⚪' -- No Evaluation Data
    END AS status,
    
    CONCAT(EXTRACT(EPOCH FROM NOW() - last_seen)::int / 60, 'm ago') AS last_seen,
    agent_version AS version,
    uptime_days || 'd' AS uptime,
    ip_address
FROM system_derivation_source
ORDER BY
    CASE 
        -- Current derivation is known and is latest
        WHEN derivation_is_known = TRUE AND current_derivation_path = latest_commit_derivation_path THEN 1 -- 🟢 Current
        
        -- Evaluating (either known or unknown derivation with pending newer evaluations)
        WHEN (derivation_is_known = TRUE AND has_pending_newer_evaluation = TRUE AND oldest_pending_eval_time >= NOW() - INTERVAL '1 hour') 
          OR (derivation_is_known = FALSE AND latest_flake_commit_timestamp > last_deployed AND has_pending_newer_evaluation = TRUE AND oldest_pending_eval_time >= NOW() - INTERVAL '1 hour') THEN 2 -- 🟡 Evaluating
        
        -- Behind (known derivation, older than latest, no pending evaluations)
        WHEN derivation_is_known = TRUE AND current_derivation_path != latest_commit_derivation_path AND has_pending_newer_evaluation = FALSE THEN 3 -- 🔴 Behind
        
        -- Unknown (various unknown states or stuck evaluations)
        WHEN derivation_is_known = FALSE 
          OR oldest_pending_eval_time < NOW() - INTERVAL '1 hour' THEN 4 -- 🟤 Unknown
        
        -- No evaluation data
        WHEN latest_commit_derivation_path IS NULL THEN 5 -- ⚪ No Evaluation
        
        -- No system state
        WHEN current_derivation_path IS NULL THEN 6 -- ⚫ No System State
        
        ELSE 7
    END,
    last_seen DESC;
