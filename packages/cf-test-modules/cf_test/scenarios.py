from typing import Any, Dict, Tuple

from . import CFTestClient


def _one_row(client: CFTestClient, sql: str, params: Tuple[Any, ...]) -> Dict[str, Any]:
    rows = client.execute_sql(sql, params)
    return rows[0] if rows else {}


# -------------------------
# SCENARIOS (CTE, atomic)
# -------------------------


def scenario_never_seen(
    client: CFTestClient, hostname: str = "test-never-seen"
) -> Dict[str, Any]:
    row = _one_row(
        client,
        """
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES (%s, %s)
          RETURNING id
        ),
        sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl.id, TRUE, %s, 'fake-key' FROM fl
          RETURNING id
        )
        SELECT (SELECT id FROM fl) AS flake_id, (SELECT id FROM sys) AS system_id
        """,
        (
            hostname,
            f"https://example.com/{hostname}.git",
            hostname,
            "/nix/store/test.drv",
        ),
    )
    return {
        "ids": row,
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
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES ('prod-app', 'https://example.com/prod.git')
          RETURNING id
        ),
        cm AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl.id, %s, NOW() - (%s)::interval, 0 FROM fl
          RETURNING id
        ),
        dv AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm.id, 'nixos', %s, %s,
                 %s, 0,
                 NOW() - ('50 minutes')::interval,
                 NOW() - ('45 minutes')::interval
          FROM cm
          RETURNING id
        ),
        sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl.id, TRUE, %s, 'fake-key' FROM fl
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
            %s, '25.05', TRUE, NOW() - ('10 minutes')::interval
          )
          RETURNING id
        ),
        hb AS (
          INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
          SELECT st.id, NOW() - ('2 minutes')::interval, %s, 'build123' FROM st
          RETURNING id
        )
        SELECT
          (SELECT id FROM fl) AS flake_id,
          (SELECT id FROM cm) AS commit_id,
          (SELECT id FROM dv) AS deriv_id,
          (SELECT id FROM st) AS state_id
        """,
        (
            "abc123current",  # cm.git_commit_hash
            "1 hour",  # cm.commit_timestamp age
            hostname,
            drv_path,  # dv.derivation_name, dv.derivation_path
            10,  # dv.status_id (complete)
            hostname,
            drv_path,  # sys.hostname, sys.derivation
            hostname,
            drv_path,  # st.hostname, st.derivation_path
            "192.168.1.100",  # st.primary_ip_address
            "2.0.0",  # hb.agent_version
        ),
    )
    return {
        "ids": row,
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
          RETURNING id
        ),
        cm_old AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl.id, %s, NOW() - ('2 days')::interval, 0 FROM fl
          RETURNING id
        ),
        cm_new AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl.id, %s, NOW() - ('1 hour')::interval, 0 FROM fl
          RETURNING id
        ),
        dv_old AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm_old.id, 'nixos', %s, %s,
                 %s, 0,
                 NOW() - ('2 days')::interval,
                 NOW() - ('47 hours')::interval
          FROM cm_old
          RETURNING id
        ),
        dv_new AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm_new.id, 'nixos', %s, %s,
                 %s, 0,
                 NOW() - ('50 minutes')::interval,
                 NOW() - ('45 minutes')::interval
          FROM cm_new
          RETURNING id
        ),
        sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl.id, TRUE, %s, 'fake-key' FROM fl
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
            %s, '25.05', TRUE, NOW() - ('5 minutes')::interval
          )
          RETURNING id
        ),
        hb AS (
          INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
          SELECT st.id, NOW() - ('1 minute')::interval, %s, 'build123' FROM st
          RETURNING id
        )
        SELECT
          (SELECT id FROM fl) AS flake_id,
          (SELECT id FROM cm_old) AS old_commit_id,
          (SELECT id FROM cm_new) AS new_commit_id,
          (SELECT id FROM dv_new) AS new_deriv_id,
          (SELECT id FROM st) AS state_id
        """,
        (
            "old456commit",  # cm_old.git_commit_hash
            "new789commit",  # cm_new.git_commit_hash
            hostname,
            old_drv,
            10,  # dv_old: name, path, status_id
            hostname,
            new_drv,
            10,  # dv_new: name, path, status_id
            hostname,
            new_drv,  # systems
            hostname,
            old_drv,  # system_states derivation_path=current is old_drv
            "192.168.1.101",  # ip
            "2.0.0",  # heartbeat version
        ),
    )
    return {
        "paths": {"old": old_drv, "new": new_drv},
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
          RETURNING id
        ),
        cm AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl.id, %s, NOW() - ('2 hours')::interval, 0 FROM fl
          RETURNING id
        ),
        dv AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm.id, 'nixos', %s, %s,
                 %s, 0,
                 NOW() - ('90 minutes')::interval,
                 NOW() - ('90 minutes')::interval
          FROM cm
          RETURNING id
        ),
        sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl.id, TRUE, %s, 'fake-key' FROM fl
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
            %s, '25.05', TRUE, NOW() - ('45 minutes')::interval
          )
          RETURNING id
        )
        SELECT
          (SELECT id FROM fl) AS flake_id,
          (SELECT id FROM cm) AS commit_id,
          (SELECT id FROM dv) AS deriv_id,
          (SELECT id FROM st) AS state_id
        """,
        (
            "offline123",
            hostname,
            drv_path,
            10,
            hostname,
            drv_path,
            hostname,
            drv_path,
            "192.168.1.102",
        ),
    )
    return {
        "ids": row,
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
    old_drv = f"/nix/store/working12-nixos-system-{hostname}.drv"
    row = _one_row(
        client,
        """
        WITH fl AS (
          INSERT INTO public.flakes (name, repo_url)
          VALUES ('failed-app', 'https://example.com/failed.git')
          RETURNING id
        ),
        cm_old AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl.id, %s, NOW() - ('1 day')::interval, 0 FROM fl
          RETURNING id
        ),
        cm_new AS (
          INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
          SELECT fl.id, %s, NOW() - ('30 minutes')::interval, 0 FROM fl
          RETURNING id
        ),
        dv_ok AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
          )
          SELECT cm_old.id, 'nixos', %s, %s,
                 %s, 0,
                 NOW() - ('20 hours')::interval,
                 NOW() - ('20 hours')::interval
          FROM cm_old
          RETURNING id
        ),
        dv_fail AS (
          INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, status_id, completed_at, error_message, attempt_count
          )
          SELECT cm_new.id, 'nixos', %s, %s, NOW() - ('30 minutes')::interval, 'Build failed', 0
          FROM cm_new
          RETURNING id
        ),
        sys AS (
          INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
          SELECT %s, fl.id, TRUE, %s, 'fake-key' FROM fl
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
            %s, '25.05', TRUE, NOW() - ('5 minutes')::interval
          )
          RETURNING id
        ),
        hb AS (
          INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
          SELECT st.id, NOW() - ('2 minutes')::interval, %s, 'build123' FROM st
          RETURNING id
        )
        SELECT
          (SELECT id FROM fl) AS flake_id,
          (SELECT id FROM cm_old) AS old_commit_id,
          (SELECT id FROM cm_new) AS new_commit_id,
          (SELECT id FROM st)     AS state_id
        """,
        (
            "working123",  # cm_old
            "broken456",  # cm_new
            hostname,
            old_drv,
            10,  # dv_ok: name, path, status=complete
            hostname,
            6,  # dv_fail: name, status=failed (6)
            hostname,
            old_drv,  # systems
            hostname,
            old_drv,  # state
            "192.168.1.103",  # ip
            "2.0.0",  # hb.agent_version
        ),
    )
    return {
        "ids": row,
        "cleanup": {
            "agent_heartbeats": [f"system_state_id = {row['state_id']}"],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": [f"id IN ({row['old_commit_id']}, {row['new_commit_id']})"],
            "flakes": [f"id = {row['flake_id']}"],
        },
    }
