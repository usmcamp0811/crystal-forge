# Helper builders for common system states (never_seen, up_to_date, behind, offline, eval_failed)
from typing import Any, Dict, Tuple

from . import CFTestClient


def _insert_one(client: CFTestClient, sql: str, params: Tuple[Any, ...]) -> int:
    return client.execute_sql(sql, params)[0]["id"]


def mk_flake(client: CFTestClient, name: str, url: str) -> int:
    return _insert_one(
        client,
        """
        INSERT INTO flakes (name, repo_url, created_at, updated_at)
        VALUES (%s, %s, NOW(), NOW()) RETURNING id
    """,
        (name, url),
    )


def mk_commit(
    client: CFTestClient, flake_id: int, hash_: str, age: str = "1 hour"
) -> int:
    return _insert_one(
        client,
        """
        INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
        VALUES (%s, %s, NOW() - INTERVAL %s, 0) RETURNING id
    """,
        (flake_id, hash_, age),
    )


def mk_derivation(
    client: CFTestClient,
    commit_id: int,
    name: str,
    path: str,
    status_id: int = 10,
    sched_age: str = "50 minutes",
    done_age: str = "45 minutes",
) -> int:
    return _insert_one(
        client,
        """
        INSERT INTO derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
        )
        VALUES (%s, 'nixos', %s, %s, %s, 0, NOW() - INTERVAL %s, NOW() - INTERVAL %s)
        RETURNING id
    """,
        (commit_id, name, path, status_id, sched_age, done_age),
    )


def mk_system(
    client: CFTestClient, hostname: str, flake_id: int, drv_path: str
) -> None:
    client.execute_sql(
        """
        INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
        VALUES (%s, %s, true, %s, 'fake-key')
    """,
        (hostname, flake_id, drv_path),
    )


def mk_state(
    client: CFTestClient,
    hostname: str,
    drv_path: str,
    ip: str,
    ts_age: str = "10 minutes",
) -> int:
    return _insert_one(
        client,
        """
        INSERT INTO system_states (
            hostname, change_reason, derivation_path, os, kernel,
            memory_gb, uptime_secs, cpu_brand, cpu_cores,
            primary_ip_address, nixos_version, agent_compatible, timestamp
        )
        VALUES (%s, 'heartbeat', %s, 'NixOS', '6.6.89',
                32.0, 3600, 'Intel Xeon', 16, %s, '25.05', true,
                NOW() - INTERVAL %s)
        RETURNING id
    """,
        (hostname, drv_path, ip, ts_age),
    )


def mk_heartbeat(
    client: CFTestClient,
    state_id: int,
    age: str = "2 minutes",
    agent_ver: str = "2.0.0",
) -> None:
    client.execute_sql(
        """
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
        VALUES (%s, NOW() - INTERVAL %s, %s, 'build123')
    """,
        (state_id, age, agent_ver),
    )


def scenario_never_seen(
    client: CFTestClient, hostname="test-never-seen", flake="test-app"
) -> Dict[str, Any]:
    flake_id = mk_flake(client, flake, f"https://example.com/{flake}.git")
    mk_system(client, hostname, flake_id, "/nix/store/test.drv")
    return {
        "cleanup": {
            "systems": [f"hostname = '{hostname}'"],
            "flakes": [f"id = {flake_id}"],
        }
    }


def scenario_up_to_date(
    client: CFTestClient, hostname="test-uptodate"
) -> Dict[str, Any]:
    flake_id = mk_flake(client, "prod-app", "https://example.com/prod.git")
    commit_id = mk_commit(client, flake_id, "abc123current", "1 hour")
    drv_path = f"/nix/store/abc123cu-nixos-system-{hostname}.drv"
    deriv_id = mk_derivation(client, commit_id, hostname, drv_path)
    mk_system(client, hostname, flake_id, drv_path)
    state_id = mk_state(client, hostname, drv_path, "192.168.1.100", "10 minutes")
    mk_heartbeat(client, state_id, "2 minutes")
    return {
        "ids": locals(),
        "cleanup": {
            "agent_heartbeats": [f"system_state_id = {state_id}"],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"id = {deriv_id}"],
            "commits": [f"id = {commit_id}"],
            "flakes": [f"id = {flake_id}"],
        },
    }


def scenario_behind(client: CFTestClient, hostname="test-behind") -> Dict[str, Any]:
    flake_id = mk_flake(client, "behind-app", "https://example.com/behind.git")
    old_commit_id = mk_commit(client, flake_id, "old456commit", "2 days")
    new_commit_id = mk_commit(client, flake_id, "new789commit", "1 hour")
    old_drv = f"/nix/store/old456co-nixos-system-{hostname}.drv"
    new_drv = f"/nix/store/new789co-nixos-system-{hostname}.drv"
    mk_derivation(client, old_commit_id, hostname, old_drv, done_age="47 hours")
    new_deriv_id = mk_derivation(
        client,
        new_commit_id,
        hostname,
        new_drv,
        sched_age="50 minutes",
        done_age="45 minutes",
    )
    mk_system(client, hostname, flake_id, old_drv)
    state_id = mk_state(client, hostname, old_drv, "192.168.1.101", "5 minutes")
    mk_heartbeat(client, state_id, "1 minute")
    return {
        "paths": {"old": old_drv, "new": new_drv},
        "cleanup": {
            "agent_heartbeats": [f"system_state_id = {state_id}"],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": [f"id IN ({old_commit_id}, {new_commit_id})"],
            "flakes": [f"id = {flake_id}"],
        },
    }


def scenario_offline(client: CFTestClient, hostname="test-offline") -> Dict[str, Any]:
    flake_id = mk_flake(client, "offline-app", "https://example.com/offline.git")
    commit_id = mk_commit(client, flake_id, "offline123", "2 hours")
    drv_path = f"/nix/store/offline12-nixos-system-{hostname}.drv"
    client.execute_sql(
        """
        INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, completed_at)
        VALUES (%s, 'nixos', %s, %s, 10, NOW() - INTERVAL '90 minutes')
    """,
        (commit_id, hostname, drv_path),
    )
    mk_system(client, hostname, flake_id, drv_path)
    state_id = mk_state(client, hostname, drv_path, "192.168.1.102", "45 minutes")
    mk_heartbeat(client, state_id, "35 minutes")
    return {
        "cleanup": {
            "agent_heartbeats": [f"system_state_id = {state_id}"],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": [f"id = {commit_id}"],
            "flakes": [f"id = {flake_id}"],
        }
    }


def scenario_eval_failed(
    client: CFTestClient, hostname="test-eval-failed"
) -> Dict[str, Any]:
    flake_id = mk_flake(client, "failed-app", "https://example.com/failed.git")
    old_commit_id = mk_commit(client, flake_id, "working123", "1 day")
    new_commit_id = mk_commit(client, flake_id, "broken456", "30 minutes")
    old_drv = f"/nix/store/working12-nixos-system-{hostname}.drv"
    client.execute_sql(
        """
        INSERT INTO derivations (commit_id, derivation_type, derivation_name, derivation_path, status_id, completed_at)
        VALUES (%s, 'nixos', %s, %s, 10, NOW() - INTERVAL '20 hours')
    """,
        (old_commit_id, hostname, old_drv),
    )
    # failed latest
    failed_id = _insert_one(
        client,
        """
        INSERT INTO derivations (commit_id, derivation_type, derivation_name, status_id, completed_at, error_message)
        VALUES (%s, 'nixos', %s, 6, NOW() - INTERVAL '30 minutes', 'Build failed') RETURNING id
    """,
        (new_commit_id, hostname),
    )
    mk_system(client, hostname, flake_id, old_drv)
    state_id = mk_state(client, hostname, old_drv, "192.168.1.103", "5 minutes")
    mk_heartbeat(client, state_id, "2 minutes")
    return {
        "cleanup": {
            "agent_heartbeats": [f"system_state_id = {state_id}"],
            "system_states": [f"hostname = '{hostname}'"],
            "systems": [f"hostname = '{hostname}'"],
            "derivations": [f"derivation_name = '{hostname}'"],
            "commits": [f"id IN ({old_commit_id}, {new_commit_id})"],
            "flakes": [f"id = {flake_id}"],
        }
    }
