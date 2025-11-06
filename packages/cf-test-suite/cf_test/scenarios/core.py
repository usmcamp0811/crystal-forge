from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING, Any, Dict, List, Optional, Tuple

if TYPE_CHECKING:
    from .. import CFTestClient


def _one_row(client: CFTestClient, sql: str, params: Tuple[Any, ...]) -> Dict[str, Any]:
    rows = client.execute_sql(sql, params)
    return rows[0] if rows else {}


def _cleanup_fn(client: CFTestClient, patterns: Dict[str, List[str]]):
    """Return a callable that cleans up using CFTestClient.cleanup_test_data()."""
    return lambda: client.cleanup_test_data(patterns)


def _create_base_scenario(
    client: CFTestClient,
    *,
    hostname: str,
    flake_name: str,
    repo_url: str,
    git_hash: str,
    commit_age_hours: int = 1,
    derivation_status: str = "dry-run-pending",
    derivation_error: Optional[str] = None,
    heartbeat_age_minutes: Optional[int] = 5,
    system_ip: str = "192.168.1.100",
    agent_version: str = "2.0.0",
    additional_commits: List[Dict[str, Any]] = None,
) -> Dict[str, Any]:
    """
    Base scenario builder that creates the standard flake -> commit -> derivation -> system -> state chain.

    Args:
        hostname: System hostname
        flake_name: Logical name for the flake (stored as flakes.name)
        repo_url: Git repository URL (unique)
        git_hash: Git commit hash
        commit_age_hours: How many hours ago the commit was made
        derivation_status: Status name ('build-complete', 'build-failed', etc.)
        derivation_error: Error message if status is 'failed'
        heartbeat_age_minutes: How many minutes ago last heartbeat (None = no heartbeat)
        system_ip: IP address for the system
        agent_version: Agent version string
        additional_commits: List of additional commits to create (for multi-commit scenarios)
    """
    now = datetime.now(UTC)
    commit_ts = now - timedelta(hours=commit_age_hours)
    drv_path = f"/nix/store/{git_hash[:12]}-nixos-system-{hostname}.drv"

    # Get status ID
    status_rows = client.execute_sql(
        "SELECT id FROM public.derivation_statuses WHERE name = %s",
        (derivation_status,),
    )
    if not status_rows:
        raise ValueError(f"Unknown derivation status: {derivation_status}")
    status_id = status_rows[0]["id"]

    # Insert flake (schema uses 'name', not 'flake_name')
    flake_row = _one_row(
        client,
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE
        SET name = EXCLUDED.name
        RETURNING id
        """,
        (flake_name, repo_url),
    )
    flake_id = flake_row["id"]

    # Insert commit
    commit_row = _one_row(
        client,
        """
        INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
        VALUES (%s, %s, %s, 0)
        ON CONFLICT (flake_id, git_commit_hash) DO UPDATE 
        SET commit_timestamp = EXCLUDED.commit_timestamp
        RETURNING id
        """,
        (flake_id, git_hash, commit_ts),
    )
    commit_id = commit_row["id"]

    # Insert additional commits if specified
    additional_commit_ids = []
    if additional_commits:
        for extra_commit in additional_commits:
            extra_ts = now - timedelta(hours=extra_commit.get("age_hours", 2))
            extra_row = _one_row(
                client,
                """
                INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
                VALUES (%s, %s, %s, 0)
                ON CONFLICT (flake_id, git_commit_hash) DO UPDATE 
                SET commit_timestamp = EXCLUDED.commit_timestamp
                RETURNING id
                """,
                (flake_id, extra_commit["hash"], extra_ts),
            )
            additional_commit_ids.append(extra_row["id"])

    # Insert derivation
    scheduled_at = commit_ts + timedelta(minutes=1)
    completed_at = (
        commit_ts + timedelta(minutes=2)
        if derivation_status == "build-complete"
        else commit_ts + timedelta(minutes=3)
    )
    deriv_row = _one_row(
        client,
        """
        INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path, store_path,
            status_id, attempt_count, scheduled_at, completed_at, error_message
        )
        VALUES (%s, 'nixos', %s, %s, %s, %s, 0, %s, %s, %s)
        RETURNING id
        """,
        (
            commit_id,
            hostname,
            drv_path,
            drv_path,  # Use same path as store_path for test scenarios
            status_id,
            scheduled_at,
            completed_at,
            derivation_error,
        ),
    )
    deriv_id = deriv_row["id"]

    # Insert system
    system_drv = drv_path if drv_path else f"/nix/store/fallback-{hostname}.drv"
    system_row = _one_row(
        client,
        """
        INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
        VALUES (%s, %s, TRUE, %s, 'fake-key')
        ON CONFLICT (hostname) DO UPDATE
        SET flake_id = EXCLUDED.flake_id,
            derivation = EXCLUDED.derivation,
            is_active = EXCLUDED.is_active
        RETURNING id
        """,
        (hostname, flake_id, system_drv),
    )
    system_id = system_row["id"]

    # Insert system state
    state_ts = commit_ts + timedelta(minutes=15)
    state_row = _one_row(
        client,
        """
        INSERT INTO public.system_states (
            hostname, change_reason, store_path, os, kernel,
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
        (hostname, system_drv, system_ip, state_ts),
    )
    state_id = state_row["id"]

    # Insert heartbeat if requested
    heartbeat_id = None
    if heartbeat_age_minutes is not None:
        heartbeat_ts = now - timedelta(minutes=heartbeat_age_minutes)
        heartbeat_row = _one_row(
            client,
            """
            INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
            VALUES (%s, %s, %s, 'build123')
            RETURNING id
            """,
            (state_id, heartbeat_ts, agent_version),
        )
        heartbeat_id = heartbeat_row["id"]

    # Build cleanup patterns - correct order for foreign key constraints
    cleanup_patterns = {
        "agent_heartbeats": [f"id = {heartbeat_id}"] if heartbeat_id else [],
        "system_states": [f"hostname = '{hostname}'"],
        "derivations": [f"id = {deriv_id}"],
        "systems": [f"hostname = '{hostname}'"],
        "commits": [f"id = {commit_id}"]
        + (
            [f"id IN ({', '.join(map(str, additional_commit_ids))})"]
            if additional_commit_ids
            else []
        ),
        "flakes": [f"id = {flake_id}"],
    }

    return {
        "hostname": hostname,
        "flake_id": flake_id,
        "commit_id": commit_id,
        "derivation_id": deriv_id,
        "system_id": system_id,
        "state_id": state_id,
        "heartbeat_id": heartbeat_id,
        "additional_commit_ids": additional_commit_ids,
        "cleanup": cleanup_patterns,
        "cleanup_fn": _cleanup_fn(client, cleanup_patterns),
    }
