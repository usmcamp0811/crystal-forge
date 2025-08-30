from __future__ import annotations

from datetime import UTC, datetime, timedelta
from typing import TYPE_CHECKING, Any, Dict, List

from .core import _cleanup_fn, _create_base_scenario, _one_row

if TYPE_CHECKING:
    from .. import CFTestClient


def scenario_flake_time_series(
    client: CFTestClient,
    *,
    flake_name: str = "scenario-timeseries",
    repo_url: str = "https://example.com/scenario-timeseries.git",
    days: int = 30,
    heartbeat_interval_minutes: int = 15,
    heartbeat_hours: int = 24,
    stagger_window_minutes: int = 60,
    base_hostname: str = "test-scenario",
    agent_version: str = "2.0.0",
) -> Dict[str, Any]:
    """
    One flake, N commits over past `days`, M systems, heartbeats every interval, upgrades
    staggered within ~stagger_window_minutes of each commit. Tweak parameters above to adjust scale.
    Returns cleanup patterns.
    """
    from math import floor

    now = datetime.now(UTC)
    start = now - timedelta(days=days)
    [flake] = client.execute_sql(
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE
        SET name = EXCLUDED.name
        RETURNING id
        """,
        (flake_name, repo_url),
    )
    flake_id = flake["id"]

    # Commits: one every ~day
    commit_ids: List[int] = []
    for d in range(days):
        ts = start + timedelta(days=d, minutes=floor(d * 1.7) % stagger_window_minutes)
        [cr] = client.execute_sql(
            """
            INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, f"{flake_name}-c{d:03d}", ts),
        )
        commit_ids.append(cr["id"])

    # 5 systems staggered across commits, all with regular heartbeats
    hostnames = [f"test-timeseries-{i+1}" for i in range(5)]
    for i, hn in enumerate(hostnames):
        commit_id = commit_ids[-1 - (i % 3)]  # spread a bit
        drv = f"/nix/store/{commit_id:012d}-nixos-system-{flake_name}.drv"

        [sysrow] = client.execute_sql(
            """
            INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, TRUE, %s, 'fake-key')
            ON CONFLICT (hostname) DO UPDATE
              SET flake_id = EXCLUDED.flake_id,
                  derivation = EXCLUDED.derivation,
                  is_active = EXCLUDED.is_active
            RETURNING id
            """,
            (hn, flake_id, drv),
        )

        [st] = client.execute_sql(
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
            (hn, drv, f"10.1.0.{i+10}", now - timedelta(hours=1)),
        )
        state_id = st["id"]

        # Heartbeats over last `heartbeat_hours` hours every `heartbeat_interval_minutes`
        for minutes_ago in range(0, heartbeat_hours * 60, heartbeat_interval_minutes):
            hb_ts = now - timedelta(minutes=minutes_ago)
            client.execute_sql(
                """
                INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
                VALUES (%s, %s, %s, 'build123')
                """,
                (state_id, hb_ts, agent_version),
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


def scenario_mixed_commit_lag(client: CFTestClient) -> Dict[str, Any]:
    """
    Create four systems:
      - 2 up_to_date (online)
      - 1 behind (online)
      - 1 offline (no recent heartbeat)
    """
    base = _create_base_scenario(
        client,
        hostname="test-mixed-1",
        flake_name="mixed-app",
        repo_url="https://example.com/mixed.git",
        git_hash="mix123current",
        commit_age_hours=1,
        heartbeat_age_minutes=5,
    )
    # second up_to_date
    _create_base_scenario(
        client,
        hostname="test-mixed-2",
        flake_name="mixed-app",
        repo_url="https://example.com/mixed.git",
        git_hash="mix123current",
        commit_age_hours=1,
        heartbeat_age_minutes=7,
    )
    # one behind (online)
    _create_base_scenario(
        client,
        hostname="test-mixed-3",
        flake_name="mixed-app",
        repo_url="https://example.com/mixed.git",
        git_hash="old000behind",
        commit_age_hours=24,
        heartbeat_age_minutes=4,
    )
    # one offline
    _create_base_scenario(
        client,
        hostname="test-mixed-4",
        flake_name="mixed-app",
        repo_url="https://example.com/mixed.git",
        git_hash="mix123current",
        commit_age_hours=1,
        heartbeat_age_minutes=65,
    )

    return {
        "hostnames": ["test-mixed-1", "test-mixed-2", "test-mixed-3", "test-mixed-4"],
        "cleanup": {
            "agent_heartbeats": [
                "WHERE system_state_id IN (SELECT id FROM public.system_states WHERE hostname LIKE 'test-mixed-%')"
            ],
            "system_states": ["hostname LIKE 'test-mixed-%'"],
            "systems": ["hostname LIKE 'test-mixed-%'"],
            "derivations": ["derivation_name LIKE 'test-mixed-%'"],
            "commits": ["git_commit_hash IN ('mix123current','old000behind')"],
            "flakes": ["repo_url = 'https://example.com/mixed.git'"],
        },
        "cleanup_fn": _cleanup_fn(
            client,
            {
                "agent_heartbeats": [
                    "WHERE system_state_id IN (SELECT id FROM public.system_states WHERE hostname LIKE 'test-mixed-%')"
                ],
                "system_states": ["hostname LIKE 'test-mixed-%'"],
                "systems": ["hostname LIKE 'test-mixed-%'"],
                "derivations": ["derivation_name LIKE 'test-mixed-%'"],
                "commits": ["git_commit_hash IN ('mix123current','old000behind')"],
                "flakes": ["repo_url = 'https://example.com/mixed.git'"],
            },
        ),
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
    from hashlib import sha256

    now = datetime.now(UTC)

    # Ensure base flake (schema uses 'name')
    [flake] = client.execute_sql(
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE
        SET name = EXCLUDED.name
        RETURNING id
        """,
        (flake_name, repo_url),
    )
    flake_id = flake["id"]

    # Insert two commits, second is latest
    [complete_status] = client.execute_sql(
        "SELECT id FROM public.derivation_statuses WHERE name='complete'"
    )
    complete_status_id = complete_status["id"]

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
        commits.append({"id": cr["id"], "drv": drv_path, "ts": ts})
    latest = commits[0]

    # Create systems
    hostnames = [f"{base_hostname}-{i+1}" for i in range(num_systems)]
    system_ids = []
    for hn in hostnames:
        [sysrow] = client.execute_sql(
            """
            INSERT INTO public.systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, TRUE, %s, 'fake-key')
            ON CONFLICT (hostname) DO UPDATE
              SET flake_id = EXCLUDED.flake_id,
                  derivation = EXCLUDED.derivation,
                  is_active = EXCLUDED.is_active
            RETURNING id
            """,
            (hn, flake_id, latest["drv"]),
        )
        system_ids.append(sysrow["id"])

        # system_state
        [st] = client.execute_sql(
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
            (
                hn,
                latest["drv"],
                f"192.168.99.{(len(system_ids) % 250) + 10}",
                latest["ts"] + timedelta(minutes=15),
            ),
        )
        state_id = st["id"]

        # heartbeats
        minutes = (
            overdue_minutes if len(system_ids) <= num_overdue else ok_heartbeat_minutes
        )
        hb_ts = now - timedelta(minutes=minutes)
        client.execute_sql(
            """
            INSERT INTO public.agent_heartbeats (system_state_id, "timestamp", agent_version, agent_build_hash)
            VALUES (%s, %s, %s, 'build123')
            """,
            (state_id, hb_ts, agent_version),
        )

    # Ensure systems point at latest derivation
    latest_drv = latest["drv"]
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


def scenario_multiple_orphaned_systems(
    client: CFTestClient, base_hostname: str = "test-multi-orphaned"
) -> Dict[str, Any]:
    """Creates multiple systems all with orphaned deployments pointing to the same flake"""

    import time

    now = datetime.now(UTC)
    timestamp = int(time.time())

    # Create single flake with commits
    flake_row = _one_row(
        client,
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        """,
        (
            f"multi-orphaned-{timestamp}",
            f"https://example.com/multi-orphaned-{timestamp}.git",
        ),
    )
    flake_id = flake_row["id"]

    # Create commits that the view can see
    commit_times = [now - timedelta(hours=h) for h in [1, 3, 6]]
    commit_ids = []

    for i, commit_time in enumerate(commit_times):
        commit_row = _one_row(
            client,
            """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, f"multi-commit-{timestamp}-{i}", commit_time),
        )
        commit_ids.append(commit_row["id"])

        # Create derivation for each commit
        _one_row(
            client,
            """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (
                %s, 'nixos', %s, %s,
                (SELECT id FROM derivation_statuses WHERE name = 'complete'),
                0, %s, %s
            )
            """,
            (
                commit_row["id"],
                f"tracked-build-{i}",
                f"/nix/store/tracked-{timestamp}-{i}.drv",
                commit_time + timedelta(minutes=10),
                commit_time + timedelta(minutes=20),
            ),
        )

    # Create 5 systems all pointing to this flake but with orphaned deployments
    hostnames = [f"{base_hostname}-{i}" for i in range(5)]
    system_ids = []
    state_ids = []
    heartbeat_ids = []

    for i, hostname in enumerate(hostnames):
        # Create system pointing to the flake
        system_row = _one_row(
            client,
            """
            INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
            VALUES (%s, %s, TRUE, 'placeholder', 'multi-orphaned-key')
            RETURNING id
            """,
            (hostname, flake_id),
        )
        system_ids.append(system_row["id"])

        # Create system_state with orphaned derivation_path
        orphaned_path = f"/nix/store/orphaned-{hostname}-{timestamp}.drv"
        state_row = _one_row(
            client,
            """
            INSERT INTO system_states (
                hostname, change_reason, derivation_path, os, kernel,
                memory_gb, uptime_secs, cpu_brand, cpu_cores,
                primary_ip_address, nixos_version, agent_compatible, timestamp
            )
            VALUES (
                %s, 'startup', %s, 'NixOS', '6.6.89',
                16.0, 3600, 'Intel Xeon', 8,
                %s, '25.05', TRUE, %s
            )
            RETURNING id
            """,
            (
                hostname,
                orphaned_path,
                f"192.168.1.{200 + i}",
                now - timedelta(minutes=30 + (i * 10)),  # Staggered deployment times
            ),
        )
        state_ids.append(state_row["id"])

        # Add heartbeats to make them look active
        heartbeat_row = _one_row(
            client,
            """
            INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
            VALUES (%s, %s, '2.0.0', 'multi-orphaned-build')
            RETURNING id
            """,
            (state_row["id"], now - timedelta(minutes=5 + i)),
        )
        heartbeat_ids.append(heartbeat_row["id"])

    cleanup_patterns = {
        "agent_heartbeats": [f"id IN ({','.join(map(str, heartbeat_ids))})"],
        "system_states": [f"hostname LIKE '{base_hostname}-%'"],
        "systems": [f"hostname LIKE '{base_hostname}-%'"],
        "derivations": [f"derivation_name LIKE 'tracked-build-%'"],
        "commits": [f"id IN ({','.join(map(str, commit_ids))})"],
        "flakes": [f"id = {flake_id}"],
    }

    return {
        "hostnames": hostnames,
        "flake_id": flake_id,
        "commit_ids": commit_ids,
        "system_ids": system_ids,
        "state_ids": state_ids,
        "heartbeat_ids": heartbeat_ids,
        "cleanup": cleanup_patterns,
        "cleanup_fn": _cleanup_fn(client, cleanup_patterns),
    }
