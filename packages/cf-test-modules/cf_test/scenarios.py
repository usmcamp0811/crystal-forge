from typing import Any, Dict, Tuple

from . import CFTestClient


def _one_row(client: CFTestClient, sql: str, params: Tuple[Any, ...]) -> Dict[str, Any]:
    rows = client.execute_sql(sql, params)
    return rows[0] if rows else {}


# All builders COMMIT inside the same execute_sql() call so subsequent queries see the rows.


def scenario_never_seen(
    client: CFTestClient, hostname: str = "test-never-seen"
) -> Dict[str, Any]:
    row = _one_row(
        client,
        """
        -- insert flake + system, then COMMIT; finally select ids
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES (%s, %s)
          ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
          RETURNING id
        ),
        sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl.id, TRUE, %s, 'fake-key' FROM fl
          ON CONFLICT (hostname) DO UPDATE
            SET flake_id = EXCLUDED.flake_id,
                derivation = EXCLUDED.derivation,
                is_active = EXCLUDED.is_active,
                public_key = EXCLUDED.public_key
          RETURNING id
        )
        SELECT 1;
        COMMIT;
        SELECT s.id AS system_id, f.id AS flake_id
        FROM public.systems s
        JOIN public.flakes f ON f.id = s.flake_id
        WHERE s.hostname = %s
        """,
        (
            hostname,
            f"https://example.com/{hostname}.git",
            hostname,
            "/nix/store/test.drv",
            hostname,
        ),
    )
    return {
        "hostname": hostname,
        "cleanup": {
            "systems": [f"hostname = '{hostname}'"],
            "flakes": [f"id = {row['flake_id']}"] if row else [],
        },
    }


def scenario_up_to_date(
    client: CFTestClient, hostname: str = "test-uptodate"
) -> Dict[str, Any]:
    drv_path = f"/nix/store/abc123cu-nixos-system-{hostname}.drv"
    row = _one_row(
        client,
        """
        -- flake, commit, derivation, system, state, heartbeat
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES ('prod-app', 'https://example.com/prod.git')
          ON CONFLICT (repo_url) DO NOTHING
          RETURNING id
        ), fl2 AS (
          SELECT COALESCE((SELECT id FROM fl), (SELECT id FROM public.flakes WHERE repo_url='https://example.com/prod.git')) AS id
        ), cm AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl2.id, %s, NOW() - (%s)::interval, 0 FROM fl2
          RETURNING id
        ), dv AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm.id, 'nixos', %s, %s,
                 10, 0,
                 NOW() - ('50 minutes')::interval,
                 NOW() - ('45 minutes')::interval
          FROM cm
          RETURNING id
        ), sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl2.id, TRUE, %s, 'fake-key' FROM fl2
          ON CONFLICT (hostname) DO UPDATE
            SET flake_id = EXCLUDED.flake_id,
                derivation = EXCLUDED.derivation,
                is_active = EXCLUDED.is_active
          RETURNING id
        ), st AS (
          INSERT INTO public.system_states (
            hostname, change_reason, derivation_path, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible, "timestamp"
          )
          VALUES (
            %s, 'startup', %s, 'NixOS', '6.6.89',
            32.0, 3600, 'Intel Xeon', 16,
            %s, '25.05', TRUE, NOW() - ('10 minutes')::interval
          )
          RETURNING id
        ), hb AS (
          INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
          SELECT st.id, NOW() - ('2 minutes')::interval, %s, 'build123' FROM st
          RETURNING id
        )
        SELECT 1;
        COMMIT;
        SELECT
          (SELECT id FROM public.flakes WHERE repo_url='https://example.com/prod.git') AS flake_id,
          (SELECT id FROM public.commits WHERE git_commit_hash=%s ORDER BY id DESC LIMIT 1) AS commit_id,
          (SELECT id FROM public.derivations WHERE derivation_name=%s ORDER BY id DESC LIMIT 1) AS deriv_id,
          (SELECT id FROM public.system_states WHERE hostname=%s ORDER BY id DESC LIMIT 1) AS state_id
        """,
        (
            "abc123current",
            "1 hour",
            hostname,
            drv_path,
            hostname,
            drv_path,
            hostname,
            drv_path,
            "192.168.1.100",
            "2.0.0",
            "abc123current",
            hostname,
            hostname,
        ),
    )
    return {
        "hostname": hostname,
        "cleanup": {
            "agent_heartbeats": [f"system_state_id = {row['state_id']}"],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"id = {row['deriv_id']}"],
            "commits": [f"id = {row['commit_id']}"],
            "flakes": [f"id = {row['flake_id']}"],
        },
    }


def scenario_behind(
    client: CFTestClient, hostname: str = "test-behind"
) -> Dict[str, Any]:
    old_drv = f"/nix/store/old456co-nixos-system-{hostname}.drv"
    new_drv = f"/nix/store/new789co-nixos-system-{hostname}.drv"
    row = _one_row(
        client,
        """
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES ('behind-app', 'https://example.com/behind.git')
          ON CONFLICT (repo_url) DO NOTHING
          RETURNING id
        ), fl2 AS (
          SELECT COALESCE((SELECT id FROM fl), (SELECT id FROM public.flakes WHERE repo_url='https://example.com/behind.git')) AS id
        ), cm_old AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl2.id, %s, NOW() - ('2 days')::interval, 0 FROM fl2
          RETURNING id
        ), cm_new AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl2.id, %s, NOW() - ('1 hour')::interval, 0 FROM fl2
          RETURNING id
        ), dv_old AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm_old.id, 'nixos', %s, %s,
                 10, 0,
                 NOW() - ('2 days')::interval,
                 NOW() - ('47 hours')::interval
          FROM cm_old
          RETURNING id
        ), dv_new AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm_new.id, 'nixos', %s, %s,
                 10, 0,
                 NOW() - ('50 minutes')::interval,
                 NOW() - ('45 minutes')::interval
          FROM cm_new
          RETURNING id
        ), sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl2.id, TRUE, %s, 'fake-key' FROM fl2
          ON CONFLICT (hostname) DO UPDATE
            SET flake_id = EXCLUDED.flake_id,
                derivation = EXCLUDED.derivation
          RETURNING id
        ), st AS (
          INSERT INTO public.system_states (
            hostname, change_reason, derivation_path, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible, "timestamp"
          )
          VALUES (
            %s, 'startup', %s, 'NixOS', '6.6.89',
            32.0, 3600, 'Intel Xeon', 16,
            %s, '25.05', TRUE, NOW() - ('5 minutes')::interval
          )
          RETURNING id
        ), hb AS (
          INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
          SELECT st.id, NOW() - ('1 minute')::interval, %s, 'build123' FROM st
          RETURNING id
        )
        SELECT 1;
        COMMIT;
        SELECT
          (SELECT id FROM public.flakes WHERE repo_url='https://example.com/behind.git') AS flake_id,
          (SELECT id FROM public.commits WHERE git_commit_hash=%s ORDER BY id DESC LIMIT 1) AS old_commit_id,
          (SELECT id FROM public.commits WHERE git_commit_hash=%s ORDER BY id DESC LIMIT 1) AS new_commit_id,
          (SELECT id FROM public.derivations WHERE derivation_name=%s AND derivation_path=%s ORDER BY id DESC LIMIT 1) AS new_deriv_id,
          (SELECT id FROM public.system_states WHERE hostname=%s ORDER BY id DESC LIMIT 1) AS state_id
        """,
        (
            "old456commit",
            "new789commit",
            hostname,
            old_drv,
            hostname,
            new_drv,
            hostname,
            new_drv,
            hostname,
            old_drv,
            "192.168.1.101",
            "2.0.0",
            "old456commit",
            "new789commit",
            hostname,
            new_drv,
            hostname,
        ),
    )
    return {
        "hostname": hostname,
        "cleanup": {
            "agent_heartbeats": [f"system_state_id = {row['state_id']}"],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": [f"id IN ({row['old_commit_id']}, {row['new_commit_id']})"],
            "flakes": [f"id = {row['flake_id']}"],
        },
    }


def scenario_offline(
    client: CFTestClient, hostname: str = "test-offline"
) -> Dict[str, Any]:
    drv_path = f"/nix/store/offline12-nixos-system-{hostname}.drv"
    row = _one_row(
        client,
        """
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES ('offline-app', 'https://example.com/offline.git')
          ON CONFLICT (repo_url) DO NOTHING
          RETURNING id
        ), fl2 AS (
          SELECT COALESCE((SELECT id FROM fl), (SELECT id FROM public.flakes WHERE repo_url='https://example.com/offline.git')) AS id
        ), cm AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl2.id, %s, NOW() - ('2 hours')::interval, 0 FROM fl2
          RETURNING id
        ), dv AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm.id, 'nixos', %s, %s,
                 10, 0,
                 NOW() - ('90 minutes')::interval,
                 NOW() - ('90 minutes')::interval
          FROM cm
          RETURNING id
        ), sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl2.id, TRUE, %s, 'fake-key' FROM fl2
          ON CONFLICT (hostname) DO UPDATE
            SET flake_id = EXCLUDED.flake_id,
                derivation = EXCLUDED.derivation
          RETURNING id
        ), st AS (
          INSERT INTO public.system_states (
            hostname, change_reason, derivation_path, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible, "timestamp"
          )
          VALUES (
            %s, 'startup', %s, 'NixOS', '6.6.89',
            32.0, 3600, 'Intel Xeon', 16,
            %s, '25.05', TRUE, NOW() - ('45 minutes')::interval
          )
          RETURNING id
        )
        SELECT 1;
        COMMIT;
        SELECT
          (SELECT id FROM public.flakes WHERE repo_url='https://example.com/offline.git') AS flake_id,
          (SELECT id FROM public.commits WHERE git_commit_hash=%s ORDER BY id DESC LIMIT 1) AS commit_id,
          (SELECT id FROM public.derivations WHERE derivation_name=%s AND derivation_path=%s ORDER BY id DESC LIMIT 1) AS deriv_id,
          (SELECT id FROM public.system_states WHERE hostname=%s ORDER BY id DESC LIMIT 1) AS state_id
        """,
        (
            "offline123",
            hostname,
            drv_path,
            hostname,
            drv_path,
            hostname,
            drv_path,
            "192.168.1.102",
            "offline123",
            hostname,
            drv_path,
            hostname,
        ),
    )
    return {
        "hostname": hostname,
        "cleanup": {
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": [f"id = {row['commit_id']}"],
            "flakes": [f"id = {row['flake_id']}"],
        },
    }


def scenario_eval_failed(
    client: CFTestClient, hostname: str = "test-eval-failed"
) -> Dict[str, Any]:
    """Create a scenario where evaluation failed for the latest commit"""

    old_drv = f"/nix/store/working12-nixos-system-{hostname}.drv"

    # Use a single transaction for all inserts to avoid isolation issues
    with client.db_connection() as conn:
        with conn.cursor() as cur:
            # Step 1: Insert flake
            cur.execute(
                """
                INSERT INTO public.flakes (name, repo_url)
                VALUES ('failed-app', 'https://example.com/failed.git')
                ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
                RETURNING id
            """
            )
            flake_result = cur.fetchone()
            if not flake_result:
                cur.execute(
                    "SELECT id FROM public.flakes WHERE repo_url = 'https://example.com/failed.git'"
                )
                flake_result = cur.fetchone()
            flake_id = flake_result["id"]

            # Step 2: Insert commits
            cur.execute(
                """
                INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
                VALUES (%s, 'working123', NOW() - INTERVAL '1 day', 0)
                ON CONFLICT (flake_id, git_commit_hash) DO UPDATE SET commit_timestamp = EXCLUDED.commit_timestamp
                RETURNING id
            """,
                (flake_id,),
            )
            old_commit_result = cur.fetchone()
            if not old_commit_result:
                cur.execute(
                    "SELECT id FROM public.commits WHERE flake_id = %s AND git_commit_hash = 'working123'",
                    (flake_id,),
                )
                old_commit_result = cur.fetchone()
            old_commit_id = old_commit_result["id"]

            cur.execute(
                """
                INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
                VALUES (%s, 'broken456', NOW() - INTERVAL '30 minutes', 0)
                ON CONFLICT (flake_id, git_commit_hash) DO UPDATE SET commit_timestamp = EXCLUDED.commit_timestamp
                RETURNING id
            """,
                (flake_id,),
            )
            new_commit_result = cur.fetchone()
            if not new_commit_result:
                cur.execute(
                    "SELECT id FROM public.commits WHERE flake_id = %s AND git_commit_hash = 'broken456'",
                    (flake_id,),
                )
                new_commit_result = cur.fetchone()
            new_commit_id = new_commit_result["id"]

            # Step 3: Insert derivations
            # Insert successful derivation for old commit
            cur.execute(
                """
                INSERT INTO public.derivations (
                    commit_id, derivation_type, derivation_name, derivation_path,
                    status_id, attempt_count, scheduled_at, completed_at
                )
                VALUES (%s, 'nixos', %s, %s, 
                        (SELECT id FROM public.derivation_statuses WHERE name='complete'),
                        0, NOW() - INTERVAL '20 hours', NOW() - INTERVAL '20 hours')
                RETURNING id
            """,
                (old_commit_id, hostname, old_drv),
            )
            old_deriv_result = cur.fetchone()
            old_deriv_id = old_deriv_result["id"] if old_deriv_result else None

            # Insert failed derivation for new commit
            cur.execute(
                """
                INSERT INTO public.derivations (
                    commit_id, derivation_type, derivation_name, derivation_path, status_id,
                    completed_at, error_message, attempt_count
                )
                VALUES (%s, 'nixos', %s, NULL,
                        (SELECT id FROM public.derivation_statuses WHERE name='failed'),
                        NOW() - INTERVAL '30 minutes', 'Evaluation failed', 0)
                RETURNING id
            """,
                (new_commit_id, hostname),
            )
            new_deriv_result = cur.fetchone()
            new_deriv_id = new_deriv_result["id"] if new_deriv_result else None

            # Step 4: Insert system (running the old working derivation)
            cur.execute(
                """
                INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
                VALUES (%s, %s, TRUE, %s, 'fake-key')
                ON CONFLICT (hostname) DO UPDATE
                    SET flake_id = EXCLUDED.flake_id,
                        derivation = EXCLUDED.derivation,
                        is_active = EXCLUDED.is_active
                RETURNING id
            """,
                (hostname, flake_id, old_drv),
            )
            system_result = cur.fetchone()
            system_id = system_result["id"] if system_result else None

            # Step 5: Insert system state
            cur.execute(
                """
                INSERT INTO public.system_states (
                    hostname, change_reason, derivation_path, os, kernel,
                    memory_gb, uptime_secs, cpu_brand, cpu_cores,
                    primary_ip_address, nixos_version, agent_compatible, "timestamp"
                )
                VALUES (
                    %s, 'startup', %s, 'NixOS', '6.6.89',
                    32.0, 3600, 'Intel Xeon', 16,
                    '192.168.1.103', '25.05', TRUE, NOW() - INTERVAL '5 minutes'
                )
                RETURNING id
            """,
                (hostname, old_drv),
            )
            state_result = cur.fetchone()
            state_id = state_result["id"] if state_result else None

            # Step 6: Insert heartbeat
            cur.execute(
                """
                INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
                VALUES (%s, NOW() - INTERVAL '2 minutes', '2.0.0', 'build123')
                RETURNING id
            """,
                (state_id,),
            )
            heartbeat_result = cur.fetchone()
            heartbeat_id = heartbeat_result["id"] if heartbeat_result else None

            # Commit all changes
            conn.commit()

    return {
        "hostname": hostname,
        "cleanup": {
            "agent_heartbeats": [f"id = {heartbeat_id}"] if heartbeat_id else [],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": [f"id IN ({old_commit_id}, {new_commit_id})"],
            "flakes": [f"id = {flake_id}"],
        },
    }
