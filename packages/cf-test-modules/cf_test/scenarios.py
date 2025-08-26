from typing import Any, Dict, List, Sequence, Tuple

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
    import time

    timestamp = int(time.time())

    old_drv = f"/nix/store/working12-nixos-system-{hostname}.drv"
    old_hash = f"working123-{timestamp}"
    new_hash = f"broken456-{timestamp}"

    row = _one_row(
        client,
        """
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES ('failed-app', 'https://example.com/failed.git')
          ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
          RETURNING id
        ),
        fl2 AS (
          SELECT COALESCE((SELECT id FROM fl), (SELECT id FROM public.flakes WHERE repo_url='https://example.com/failed.git')) AS id
        ),
        cm_old AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl2.id, %s, NOW() - INTERVAL '1 day', 0 FROM fl2
          ON CONFLICT (flake_id, git_commit_hash) DO UPDATE SET commit_timestamp = EXCLUDED.commit_timestamp
          RETURNING id
        ),
        cm_new AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl2.id, %s, NOW() - INTERVAL '30 minutes', 0 FROM fl2
          ON CONFLICT (flake_id, git_commit_hash) DO UPDATE SET commit_timestamp = EXCLUDED.commit_timestamp
          RETURNING id
        ),
        dv_old AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm_old.id, 'nixos', %s, %s,
                 (SELECT id FROM public.derivation_statuses WHERE name='complete'),
                 0, NOW() - INTERVAL '20 hours', NOW() - INTERVAL '20 hours'
          FROM cm_old
          RETURNING id
        ),
        dv_new AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path, status_id,
            completed_at, error_message, attempt_count
          )
          SELECT cm_new.id, 'nixos', %s, NULL,
                 (SELECT id FROM public.derivation_statuses WHERE name='failed'),
                 NOW() - INTERVAL '30 minutes', 'Evaluation failed', 0
          FROM cm_new
          RETURNING id
        ),
        sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl2.id, TRUE, %s, 'fake-key' FROM fl2
          ON CONFLICT (hostname) DO UPDATE
            SET flake_id = EXCLUDED.flake_id,
                derivation = EXCLUDED.derivation,
                is_active = EXCLUDED.is_active
          RETURNING id
        ),
        st AS (
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
        ),
        hb AS (
          INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
          SELECT st.id, NOW() - INTERVAL '2 minutes', '2.0.0', 'build123' FROM st
          RETURNING id
        )
        SELECT 1;
        COMMIT;
        SELECT
          (SELECT id FROM public.flakes WHERE repo_url='https://example.com/failed.git') AS flake_id,
          (SELECT id FROM public.commits WHERE git_commit_hash=%s ORDER BY id DESC LIMIT 1) AS old_commit_id,
          (SELECT id FROM public.commits WHERE git_commit_hash=%s ORDER BY id DESC LIMIT 1) AS new_commit_id,
          (SELECT id FROM public.derivations WHERE derivation_name=%s AND derivation_path=%s ORDER BY id DESC LIMIT 1) AS old_deriv_id,
          (SELECT id FROM public.derivations WHERE derivation_name=%s AND derivation_path IS NULL ORDER BY id DESC LIMIT 1) AS new_deriv_id,
          (SELECT id FROM public.system_states WHERE hostname=%s ORDER BY id DESC LIMIT 1) AS state_id
        """,
        (
            old_hash,  # cm_old git_commit_hash
            new_hash,  # cm_new git_commit_hash
            hostname,  # dv_old derivation_name
            old_drv,  # dv_old derivation_path
            hostname,  # dv_new derivation_name
            hostname,  # sys hostname
            old_drv,  # sys derivation
            hostname,  # st hostname
            old_drv,  # st derivation_path
            old_hash,  # SELECT old_commit_id
            new_hash,  # SELECT new_commit_id
            hostname,  # SELECT old_deriv_id derivation_name
            old_drv,  # SELECT old_deriv_id derivation_path
            hostname,  # SELECT new_deriv_id derivation_name
            hostname,  # SELECT state_id
        ),
    )
    return {
        "hostname": hostname,
        "cleanup": {
            "agent_heartbeats": (
                [f"system_state_id = {row['state_id']}"]
                if row and "state_id" in row
                else []
            ),
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": (
                [f"id IN ({row['old_commit_id']}, {row['new_commit_id']})"]
                if row and "old_commit_id" in row
                else [
                    f"git_commit_hash LIKE '%working123%' OR git_commit_hash LIKE '%broken456%'"
                ]
            ),
            "flakes": (
                [f"id = {row['flake_id']}"]
                if row and "flake_id" in row
                else [f"repo_url = 'https://example.com/failed.git'"]
            ),
        },
    }


def _cleanup_fn(client: CFTestClient, patterns: Dict[str, List[str]]):
    """Return a callable that cleans up using CFTestClient.cleanup_test_data()."""
    return lambda: client.cleanup_test_data(patterns)


def scenario_flake_time_series(
    client: CFTestClient,
    *,
    flake_name: str = "scenario-flake",
    repo_url: str = "https://example.com/scenario-flake.git",
    num_commits: int = 10,
    num_systems: int = 9,
    days: int = 30,
    heartbeat_interval_minutes: int = 15,
    heartbeat_hours: int = 24,
    stagger_window_minutes: int = 60,
    base_hostname: str = "test-scenario",
    agent_version: str = "2.0.0",
) -> Dict[str, Any]:
    from datetime import UTC, datetime, timedelta

    now = datetime.now(UTC)

    rows = client.execute_sql(
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        """,
        (flake_name, repo_url),
    )
    flake_id = rows[0]["id"]

    status_rows = client.execute_sql(
        "SELECT id FROM public.derivation_statuses WHERE name='complete'"
    )
    complete_status_id = status_rows[0]["id"]

    hostnames: List[str] = [f"{base_hostname}-{i+1}" for i in range(num_systems)]
    for i, hn in enumerate(hostnames):
        client.execute_sql(
            """
            INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, TRUE, %s, 'fake-key')
            ON CONFLICT (hostname) DO UPDATE
              SET flake_id = EXCLUDED.flake_id,
                  is_active = EXCLUDED.is_active
            RETURNING id
            """,
            (hn, flake_id, f"/nix/store/bootstrap-{hn}.drv"),
        )

    commit_hashes: List[str] = []
    total_hours = max(days, 1) * 24
    step = total_hours / max(num_commits - 1, 1)

    for idx in range(num_commits):
        age_hours = int(round(total_hours - idx * step))
        age_hours = max(age_hours, 1)
        commit_ts = now - timedelta(hours=age_hours)
        git_hash = f"{flake_name}-c{idx+1:02d}-{int(commit_ts.timestamp())}"
        commit_hashes.append(git_hash)

        cr = client.execute_sql(
            """
            INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, git_hash, commit_ts),
        )
        commit_id = cr[0]["id"]

        drv_path = f"/nix/store/{git_hash[:12]}-nixos-system-{flake_name}.drv"
        scheduled_at = commit_ts + timedelta(minutes=5)
        completed_at = commit_ts + timedelta(minutes=10)

        client.execute_sql(
            """
            INSERT INTO public.derivations (
              commit_id, derivation_type, derivation_name, derivation_path,
              status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, %s, 0, %s, %s)
            RETURNING id
            """,
            (
                commit_id,
                f"{flake_name}-build-{idx+1:02d}",
                drv_path,
                complete_status_id,
                scheduled_at,
                completed_at,
            ),
        )

        for sidx, hn in enumerate(hostnames):
            offset_min = int(
                round((stagger_window_minutes / max(num_systems - 1, 1)) * sidx)
            )
            upgrade_ts = commit_ts + timedelta(minutes=offset_min)

            client.execute_sql(
                """
                INSERT INTO public.system_states (
                  hostname, change_reason, derivation_path, os, kernel,
                  memory_gb, uptime_secs, cpu_brand, cpu_cores,
                  primary_ip_address, nixos_version, agent_compatible, "timestamp"
                )
                VALUES (
                  %s, 'startup', %s, 'NixOS', '6.6.89',
                  32.0, 3600, 'Intel Xeon', 16,
                  %s, '25.05', TRUE, %s
                )
                RETURNING id
                """,
                (hn, drv_path, f"192.168.50.{100 + sidx}", upgrade_ts),
            )

        client.execute_sql(
            """
            UPDATE public.systems SET derivation = %s
            WHERE hostname = ANY(%s)
            """,
            (drv_path, hostnames),
        )

    from math import floor

    interval_minutes = heartbeat_interval_minutes
    beats_per_system = floor(heartbeat_hours * 60 / interval_minutes)

    for hn in hostnames:
        st = client.execute_sql(
            """
            SELECT id FROM public.system_states
            WHERE hostname = %s
            ORDER BY "timestamp" DESC
            LIMIT 1
            """,
            (hn,),
        )
        if not st:
            continue
        state_id = st[0]["id"]

        for k in range(beats_per_system):
            ts = now - timedelta(minutes=k * interval_minutes)
            client.execute_sql(
                """
                INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
                VALUES (%s, %s, %s, 'build123')
                """,
                (state_id, ts, agent_version),
            )

    hostname_like = f"{base_hostname}-%"
    cleanup_patterns = {
        "agent_heartbeats": [
            "WHERE system_state_id IN (SELECT id FROM public.system_states WHERE hostname LIKE '%s')"
            % hostname_like
        ],
        "system_states": [f"hostname LIKE '{hostname_like}'"],
        "systems": [f"hostname LIKE '{hostname_like}'"],
        "derivations": [f"derivation_name LIKE '{flake_name}-build-%'"],
        "commits": [f"flake_id = {flake_id}"],
        "flakes": [f"id = {flake_id}"],
    }

    return {
        "flake_id": flake_id,
        "repo_url": repo_url,
        "hostnames": hostnames,
        "commit_hashes": commit_hashes,
        "cleanup": cleanup_patterns,
        "cleanup_fn": _cleanup_fn(client, cleanup_patterns),
    }


def scenario_latest_with_two_overdue(
    client: CFTestClient,
    *,
    flake_name: str = "scenario-latest-all",
    repo_url: str = "https://example.com/scenario-latest-all.git",
    num_systems: int = 9,
    num_overdue: int = 2,
    overdue_minutes: int = 65,
    ok_heartbeat_minutes: int = 5,
    base_hostname: str = "test-latest",
    agent_version: str = "2.0.0",
) -> Dict[str, Any]:
    from datetime import UTC, datetime, timedelta
    from hashlib import sha256

    now = datetime.now(UTC)

    [flake_row] = client.execute_sql(
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        """,
        (flake_name, repo_url),
    )
    flake_id = flake_row["id"]

    [st_row] = client.execute_sql(
        "SELECT id FROM public.derivation_statuses WHERE name='complete'"
    )
    complete_status_id = st_row["id"]

    hostnames: List[str] = [f"{base_hostname}-{i+1}" for i in range(num_systems)]
    for hn in hostnames:
        client.execute_sql(
            """
            INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, TRUE, %s, 'fake-key')
            ON CONFLICT (hostname) DO UPDATE
              SET flake_id = EXCLUDED.flake_id,
                  is_active = EXCLUDED.is_active
            """,
            (hn, flake_id, f"/nix/store/bootstrap-{hn}.drv"),
        )

    commits = []
    for i, age_h in enumerate([2, 6]):
        ts = now - timedelta(hours=age_h)
        git_hash = f"{flake_name}-c{i+1:02d}-{int(ts.timestamp())}"
        [cr] = client.execute_sql(
            """
            INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, git_hash, ts),
        )
        slug = sha256(f"{git_hash}-{cr['id']}".encode()).hexdigest()[:12]
        drv_path = f"/nix/store/{slug}-nixos-system-{flake_name}.drv"

        client.execute_sql(
            """
            INSERT INTO public.derivations (
              commit_id, derivation_type, derivation_name, derivation_path,
              status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, %s, 0, %s, %s)
            """,
            (
                cr["id"],
                f"{flake_name}-build-{i+1:02d}",
                drv_path,
                complete_status_id,
                ts + timedelta(minutes=5),
                ts + timedelta(minutes=10),
            ),
        )
        commits.append((cr["id"], git_hash, drv_path, ts))

    latest_drv = commits[0][2]

    for idx, hn in enumerate(hostnames):
        upgrade_ts = commits[0][3] + timedelta(
            minutes=int((45 / max(num_systems - 1, 1)) * idx)
        )
        [state] = client.execute_sql(
            """
            INSERT INTO public.system_states (
              hostname, change_reason, derivation_path, os, kernel,
              memory_gb, uptime_secs, cpu_brand, cpu_cores,
              primary_ip_address, nixos_version, agent_compatible, "timestamp"
            )
            VALUES (%s, 'startup', %s, 'NixOS', '6.6.89',
                    32.0, 3600, 'Intel Xeon', 16,
                    %s, '25.05', TRUE, %s)
            RETURNING id
            """,
            (hn, latest_drv, f"10.0.0.{100+idx}", upgrade_ts),
        )
        heartbeat_age = overdue_minutes if idx < num_overdue else ok_heartbeat_minutes
        hb_ts = now - timedelta(minutes=heartbeat_age)
        client.execute_sql(
            """
            INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
            VALUES (%s, %s, %s, 'build123')
            """,
            (state["id"], hb_ts, agent_version),
        )

    client.execute_sql(
        "UPDATE public.systems SET derivation = %s WHERE hostname = ANY(%s)",
        (latest_drv, hostnames),
    )

    hostname_like = f"{base_hostname}-%"
    cleanup_patterns = {
        "agent_heartbeats": [
            "WHERE system_state_id IN (SELECT id FROM public.system_states WHERE hostname LIKE '%s')"
            % hostname_like
        ],
        "system_states": [f"hostname LIKE '{hostname_like}'"],
        "systems": [f"hostname LIKE '{hostname_like}'"],
        "derivations": [f"derivation_name LIKE '{flake_name}-build-%'"],
        "commits": [f"flake_id = {flake_id}"],
        "flakes": [f"id = {flake_id}"],
    }

    return {
        "flake_id": flake_id,
        "hostnames": hostnames,
        "cleanup": cleanup_patterns,
        "cleanup_fn": _cleanup_fn(client, cleanup_patterns),
    }


def scenario_mixed_commit_lag(
    client: CFTestClient,
    *,
    flake_name: str = "scenario-mixed-lag",
    repo_url: str = "https://example.com/scenario-mixed-lag.git",
    commit_lags: Sequence[int] = (0, 1, 3, 3),
    heartbeat_max_age_minutes: int = 15,
    base_hostname: str = "test-mixed",
    agent_version: str = "2.0.0",
) -> Dict[str, Any]:
    from datetime import UTC, datetime, timedelta
    from hashlib import sha256

    now = datetime.now(UTC)

    [flake_row] = client.execute_sql(
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        """,
        (flake_name, repo_url),
    )
    flake_id = flake_row["id"]

    [st_row] = client.execute_sql(
        "SELECT id FROM public.derivation_statuses WHERE name='complete'"
    )
    complete_status_id = st_row["id"]

    hostnames: List[str] = [f"{base_hostname}-{i+1}" for i in range(len(commit_lags))]
    for hn in hostnames:
        client.execute_sql(
            """
            INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, TRUE, %s, 'fake-key')
            ON CONFLICT (hostname) DO UPDATE
              SET flake_id = EXCLUDED.flake_id,
                  is_active = EXCLUDED.is_active
            """,
            (hn, flake_id, f"/nix/store/bootstrap-{hn}.drv"),
        )

    max_lag = max(commit_lags) if commit_lags else 0
    num_commits = max_lag + 1
    commits = []
    for i in range(num_commits):
        ts = now - timedelta(minutes=90 * (num_commits - 1 - i))
        git_hash = f"{flake_name}-c{i+1:02d}-{int(ts.timestamp())}"
        [cr] = client.execute_sql(
            """
            INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, git_hash, ts),
        )
        slug = sha256(f"{git_hash}-{cr['id']}".encode()).hexdigest()[:12]
        drv_path = f"/nix/store/{slug}-nixos-system-{flake_name}.drv"

        client.execute_sql(
            """
            INSERT INTO public.derivations (
              commit_id, derivation_type, derivation_name, derivation_path,
              status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (%s, 'nixos', %s, %s, %s, 0, %s, %s)
            """,
            (
                cr["id"],
                f"{flake_name}-build-{i+1:02d}",
                drv_path,
                complete_status_id,
                ts + timedelta(minutes=5),
                ts + timedelta(minutes=10),
            ),
        )
        commits.append((cr["id"], git_hash, drv_path, ts))

    drv_by_index = [c[2] for c in commits]

    for idx, (hn, lag) in enumerate(zip(hostnames, commit_lags)):
        commit_index = len(drv_by_index) - 1 - lag
        commit_index = max(0, min(commit_index, len(drv_by_index) - 1))
        drv_path = drv_by_index[commit_index]
        upgrade_ts = commits[commit_index][3] + timedelta(minutes=idx * 5)
        [state] = client.execute_sql(
            """
            INSERT INTO public.system_states (
              hostname, change_reason, derivation_path, os, kernel,
              memory_gb, uptime_secs, cpu_brand, cpu_cores,
              primary_ip_address, nixos_version, agent_compatible, "timestamp"
            )
            VALUES (%s, 'startup', %s, 'NixOS', '6.6.89',
                    32.0, 3600, 'Intel Xeon', 16,
                    %s, '25.05', TRUE, %s)
            RETURNING id
            """,
            (hn, drv_path, f"10.0.1.{100+idx}", upgrade_ts),
        )
        hb_ts = now - timedelta(minutes=min(heartbeat_max_age_minutes, 10))
        client.execute_sql(
            """
            INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
            VALUES (%s, %s, %s, 'build123')
            """,
            (state["id"], hb_ts, agent_version),
        )
        client.execute_sql(
            "UPDATE public.systems SET derivation = %s WHERE hostname = %s",
            (drv_path, hn),
        )

    hostname_like = f"{base_hostname}-%"
    cleanup_patterns = {
        "agent_heartbeats": [
            "WHERE system_state_id IN (SELECT id FROM public.system_states WHERE hostname LIKE '%s')"
            % hostname_like
        ],
        "system_states": [f"hostname LIKE '{hostname_like}'"],
        "systems": [f"hostname LIKE '{hostname_like}'"],
        "derivations": [f"derivation_name LIKE '{flake_name}-build-%'"],
        "commits": [f"flake_id = {flake_id}"],
        "flakes": [f"id = {flake_id}"],
    }

    return {
        "flake_id": flake_id,
        "hostnames": hostnames,
        "cleanup": cleanup_patterns,
        "cleanup_fn": _cleanup_fn(client, cleanup_patterns),
    }
