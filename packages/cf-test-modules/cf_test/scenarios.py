from datetime import UTC, datetime, timedelta
from typing import Any, Dict, List, Optional, Sequence, Tuple

from . import CFTestClient


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
    derivation_status: str = "complete",
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
        derivation_status: Status name ('complete', 'failed', etc.)
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
        if derivation_status == "complete"
        else commit_ts + timedelta(minutes=3)
    )
    deriv_row = _one_row(
        client,
        """
        INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at, error_message
        )
        VALUES (%s, 'nixos', %s, %s, %s, 0, %s, %s, %s)
        RETURNING id
        """,
        (
            commit_id,
            hostname,
            drv_path,
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


def scenario_never_seen(
    client: CFTestClient, hostname: str = "test-never-seen"
) -> Dict[str, Any]:
    """Pre-registered/tracked system that has **never sent a heartbeat**.

    We still create `systems` and `system_states` to represent a known, tracked
    machine (the app tracks systems before first contact), but we set
    `heartbeat_age_minutes=None` so **no agent_heartbeats are inserted**.
    Views should therefore resolve connectivity/update/overall to "never_seen".
    """
    import time

    timestamp = int(time.time())

    return _create_base_scenario(
        client,
        hostname=hostname,
        flake_name=f"{hostname}-{timestamp}",
        repo_url=f"https://example.com/{hostname}-{timestamp}.git",
        git_hash=f"never123seen-{timestamp}",
        commit_age_hours=1,
        heartbeat_age_minutes=None,  # No heartbeat = never seen
    )


def scenario_up_to_date(
    client: CFTestClient, hostname: str = "test-uptodate"
) -> Dict[str, Any]:
    """System that is up to date and online"""
    return _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="prod-app",
        repo_url="https://example.com/prod.git",
        git_hash="abc123current",
        commit_age_hours=1,
        heartbeat_age_minutes=2,
        system_ip="192.168.1.100",
    )


def scenario_behind(
    client: CFTestClient, hostname: str = "test-behind"
) -> Dict[str, Any]:
    """System that is behind the latest commit"""
    old_drv = f"/nix/store/old456co-nixos-system-{hostname}.drv"
    new_drv = f"/nix/store/new789co-nixos-system-{hostname}.drv"

    # Create base scenario with old commit
    result = _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="behind-app",
        repo_url="https://example.com/behind.git",
        git_hash="old456commit",
        commit_age_hours=48,  # Old commit from 2 days ago
        heartbeat_age_minutes=1,
        system_ip="192.168.1.101",
        additional_commits=[
            {"hash": "new789commit", "age_hours": 1}  # Newer commit available
        ],
    )

    # Update system to point to old derivation path
    client.execute_sql(
        "UPDATE public.systems SET derivation = %s WHERE hostname = %s",
        (old_drv, hostname),
    )

    return result


def scenario_offline(
    client: CFTestClient, hostname: str = "test-offline"
) -> Dict[str, Any]:
    """System that is offline (no recent heartbeats)"""
    return _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="offline-app",
        repo_url="https://example.com/offline.git",
        git_hash="offline123",
        commit_age_hours=2,
        heartbeat_age_minutes=45,  # cutoff is 30m, so this is offline
        system_ip="192.168.1.102",
    )


def scenario_eval_failed(
    client: CFTestClient, hostname: str = "test-eval-failed"
) -> Dict[str, Any]:
    """System with a failed evaluation for the latest commit"""
    import time

    timestamp = int(time.time())

    old_hash = f"working123-{timestamp}"
    new_hash = f"broken456-{timestamp}"

    # Create base scenario with working commit
    result = _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="eval-app",
        repo_url="https://example.com/eval.git",
        git_hash=old_hash,
        commit_age_hours=4,
        heartbeat_age_minutes=3,
    )

    # Insert a newer commit and failed derivation for it
    [new_commit] = client.execute_sql(
        """
        INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
        VALUES (%s, %s, %s, 0)
        RETURNING id
        """,
        (result["flake_id"], new_hash, datetime.now(UTC) - timedelta(hours=1)),
    )
    new_commit_id = new_commit["id"]

    now = datetime.now(UTC)
    failed_completed = now - timedelta(minutes=30)

    client.execute_sql(
        """
        INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, completed_at, error_message
        )
        VALUES (%s, 'nixos', %s, NULL,
                (SELECT id FROM public.derivation_statuses WHERE name='failed'),
                0, %s, 'Evaluation failed')
        """,
        (new_commit_id, f"{hostname}-build-failed", failed_completed),
    )

    return result


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

def scenario_agent_restart(
    client: CFTestClient, hostname: str = "test-agent-restart"
) -> Dict[str, Any]:
    """Agent that stops heartbeating, then resumes hours later"""
    
    now = datetime.now(UTC)
    
    # Create base system with recent deployment
    base = _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="agent-restart-test",
        repo_url="https://example.com/agent-restart.git",
        git_hash="restart-123",
        commit_age_hours=6,
        heartbeat_age_minutes=None,  # We'll create custom heartbeats
        system_ip="192.168.1.150"
    )
    
    system_state_id = base["state_id"]
    
    # Create heartbeat timeline: active, then gap, then resumed
    heartbeat_times = [
        now - timedelta(hours=4),      # 4 hours ago - normal
        now - timedelta(hours=3),      # 3 hours ago - normal  
        now - timedelta(hours=2, minutes=30),  # 2.5 hours ago - last before gap
        # Gap of 2 hours (agent offline)
        now - timedelta(minutes=15),   # 15 minutes ago - resumed
        now - timedelta(minutes=5),    # 5 minutes ago - current
    ]
    
    heartbeat_ids = []
    for i, hb_time in enumerate(heartbeat_times):
        agent_version = "2.1.0" if i >= 3 else "2.0.0"  # Version upgrade during restart
        hb_row = _one_row(
            client,
            """
            INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
            VALUES (%s, %s, %s, 'build456')
            RETURNING id
            """,
            (system_state_id, hb_time, agent_version)
        )
        heartbeat_ids.append(hb_row["id"])
    
    cleanup_patterns = base["cleanup"].copy()
    cleanup_patterns["agent_heartbeats"] = [f"id IN ({','.join(map(str, heartbeat_ids))})"]
    
    return {**base, "heartbeat_ids": heartbeat_ids, "cleanup": cleanup_patterns}


def scenario_build_timeout(
    client: CFTestClient, hostname: str = "test-build-timeout"
) -> Dict[str, Any]:
    """Commit with derivations stuck in building state for extended time"""
    
    now = datetime.now(UTC)
    commit_ts = now - timedelta(hours=8)  # Commit 8 hours ago
    
    # Create base scenario
    base = _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="build-timeout-test",
        repo_url="https://example.com/build-timeout.git", 
        git_hash="timeout-789",
        commit_age_hours=8,
        derivation_status="pending",  # Initial state
        heartbeat_age_minutes=10
    )
    
    commit_id = base["commit_id"]
    
    # Update the main derivation to be stuck building
    client.execute_sql(
        """
        UPDATE derivations 
        SET status_id = (SELECT id FROM derivation_statuses WHERE name = 'pending'),
            started_at = %s,
            completed_at = NULL
        WHERE id = %s
        """,
        (now - timedelta(hours=6), base["derivation_id"])
    )
    
    # Add additional derivations in various stuck states
    stuck_derivations = [
        ("package-1", "pending", now - timedelta(hours=4)),
        ("package-2", "pending", now - timedelta(hours=7)),
        ("package-3", "pending", now - timedelta(hours=3)),
    ]
    
    additional_deriv_ids = []
    for pkg_name, status, started_time in stuck_derivations:
        deriv_row = _one_row(
            client,
            """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, started_at
            )
            VALUES (
                %s, 'package', %s, %s,
                (SELECT id FROM derivation_statuses WHERE name = %s),
                1, %s, %s
            )
            RETURNING id
            """,
            (
                commit_id,
                f"{hostname}-{pkg_name}",
                f"/nix/store/{pkg_name}-timeout.drv",
                status,
                started_time - timedelta(minutes=10),
                started_time
            )
        )
        additional_deriv_ids.append(deriv_row["id"])
    
    cleanup_patterns = base["cleanup"].copy()
    cleanup_patterns["derivations"].append(
        f"id IN ({','.join(map(str, additional_deriv_ids))})"
    )
    
    return {**base, "additional_derivation_ids": additional_deriv_ids}


def scenario_rollback(
    client: CFTestClient, hostname: str = "test-rollback"
) -> Dict[str, Any]:
    """System that deploys newer commit, then rolls back to older one"""
    
    import time
    now = datetime.now(UTC)
    timestamp = int(time.time())
    
    # Create flake and commits with unique hashes
    flake_row = _one_row(
        client,
        """
        INSERT INTO flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        """,
        (f"rollback-test-{timestamp}", f"https://example.com/rollback-{timestamp}.git")
    )
    flake_id = flake_row["id"]
    
    # Create timeline: old commit (stable) -> new commit (problematic) -> rollback to old
    commits_data = [
        (f"old-stable-{timestamp}", now - timedelta(days=7), "complete"),
        (f"new-problem-{timestamp}", now - timedelta(hours=6), "complete"),
    ]
    
    commit_ids = []
    derivation_ids = []
    
    for git_hash, commit_time, status in commits_data:
        # Create commit
        commit_row = _one_row(
            client,
            """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, git_hash, commit_time)
        )
        commit_ids.append(commit_row["id"])
        
        # Create derivation for each commit
        deriv_path = f"/nix/store/{git_hash[:8]}-nixos-system-{hostname}.drv"
        deriv_row = _one_row(
            client,
            """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, completed_at
            )
            VALUES (
                %s, 'nixos', %s, %s,
                (SELECT id FROM derivation_statuses WHERE name = %s),
                0, %s, %s
            )
            RETURNING id
            """,
            (
                commit_row["id"],
                hostname,
                deriv_path,
                status,
                commit_time + timedelta(minutes=5),
                commit_time + timedelta(minutes=15)
            )
        )
        derivation_ids.append(deriv_row["id"])
    
    # Create system pointing to new commit initially
    system_row = _one_row(
        client,
        """
        INSERT INTO systems (hostname, flake_id, is_active, derivation, public_key)
        VALUES (%s, %s, TRUE, %s, 'rollback-key')
        RETURNING id
        """,
        (hostname, flake_id, f"/nix/store/{commits_data[1][0][:8]}-nixos-system-{hostname}.drv")
    )
    system_id = system_row["id"]
    
    # Create system state timeline: deployed new, then rolled back to old
    state_timeline = [
        (now - timedelta(hours=5), commits_data[1][0], "config_change"),  # Deploy new
        (now - timedelta(minutes=30), commits_data[0][0], "config_change"), # Rollback to old
    ]
    
    state_ids = []
    for state_time, git_hash, reason in state_timeline:
        deriv_path = f"/nix/store/{git_hash[:8]}-nixos-system-{hostname}.drv"
        state_row = _one_row(
            client,
            """
            INSERT INTO system_states (
                hostname, change_reason, derivation_path, os, kernel,
                memory_gb, uptime_secs, cpu_brand, cpu_cores,
                primary_ip_address, nixos_version, agent_compatible, timestamp
            )
            VALUES (
                %s, %s, %s, 'NixOS', '6.6.89',
                32.0, 7200, 'Intel Xeon', 16,
                '192.168.1.160', '25.05', TRUE, %s
            )
            RETURNING id
            """,
            (hostname, reason, deriv_path, state_time)
        )
        state_ids.append(state_row["id"])
    
    # Add recent heartbeats
    heartbeat_row = _one_row(
        client,
        """
        INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
        VALUES (%s, %s, '2.0.0', 'rollback-build')
        RETURNING id
        """,
        (state_ids[-1], now - timedelta(minutes=3))
    )
    
    cleanup_patterns = {
        "agent_heartbeats": [f"id = {heartbeat_row['id']}"],
        "system_states": [f"hostname = '{hostname}'"],
        "systems": [f"hostname = '{hostname}'"],
        "derivations": [f"id IN ({','.join(map(str, derivation_ids))})"],
        "commits": [f"id IN ({','.join(map(str, commit_ids))})"],
        "flakes": [f"id = {flake_id}"],
    }
    
    return {
        "hostname": hostname,
        "flake_id": flake_id,
        "commit_ids": commit_ids,
        "derivation_ids": derivation_ids,
        "system_id": system_id,
        "state_ids": state_ids,
        "cleanup": cleanup_patterns,
        "cleanup_fn": _cleanup_fn(client, cleanup_patterns),
    }


def scenario_partial_rebuild(
    client: CFTestClient, hostname: str = "test-partial-rebuild"
) -> Dict[str, Any]:
    """Commit where only some derivations get rebuilt after initial failure"""
    
    now = datetime.now(UTC)
    
    # Create base scenario
    base = _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="partial-rebuild-test",
        repo_url="https://example.com/partial-rebuild.git",
        git_hash="partial-456", 
        commit_age_hours=12,
        derivation_status="complete",  # Main derivation succeeds
        heartbeat_age_minutes=8
    )
    
    commit_id = base["commit_id"]
    
    # Add packages with mixed success/failure/retry pattern
    package_scenarios = [
        ("pkg-success", "complete", 1, now - timedelta(hours=11)),
        ("pkg-failed-once", "failed", 2, now - timedelta(hours=10)),
        ("pkg-retry-success", "complete", 3, now - timedelta(hours=9)),
        ("pkg-still-failing", "failed", 4, now - timedelta(hours=8)),
        ("pkg-building", "pending", 2, now - timedelta(minutes=30)),
    ]
    
    package_deriv_ids = []
    for pkg_name, final_status, attempts, last_attempt_time in package_scenarios:
        deriv_row = _one_row(
            client,
            """
            INSERT INTO derivations (
                commit_id, derivation_type, derivation_name, derivation_path,
                status_id, attempt_count, scheduled_at, started_at, completed_at,
                error_message
            )
            VALUES (
                %s, 'package', %s, %s,
                (SELECT id FROM derivation_statuses WHERE name = %s),
                %s, %s, %s, %s, %s
            )
            RETURNING id
            """,
            (
                commit_id,
                f"{hostname}-{pkg_name}",
                f"/nix/store/{pkg_name}-rebuild.drv", 
                final_status,
                attempts,
                now - timedelta(hours=12),
                last_attempt_time,
                last_attempt_time + timedelta(minutes=15) if final_status in ["complete", "failed"] else None,
                f"Build failed after {attempts} attempts" if final_status == "failed" else None
            )
        )
        package_deriv_ids.append(deriv_row["id"])
    
    cleanup_patterns = base["cleanup"].copy()
    cleanup_patterns["derivations"].append(
        f"id IN ({','.join(map(str, package_deriv_ids))})"
    )
    
    return {**base, "package_derivation_ids": package_deriv_ids}


def scenario_compliance_drift(
    client: CFTestClient, hostname: str = "test-compliance-drift"
) -> Dict[str, Any]:
    """System that hasn't updated in 30+ days despite newer commits available"""
    
    now = datetime.now(UTC)
    
    # Create old deployment (45 days behind)
    old_deployment = _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="compliance-drift-test", 
        repo_url="https://example.com/compliance-drift.git",
        git_hash="ancient-commit-123",
        commit_age_hours=24 * 45,  # 45 days old
        heartbeat_age_minutes=12   # System is online but old
    )
    
    flake_id = old_deployment["flake_id"]
    
    # Create many newer commits (simulate active development)
    recent_commits = []
    for days_ago in [30, 20, 15, 10, 7, 3, 1]:
        commit_time = now - timedelta(days=days_ago)
        commit_row = _one_row(
            client,
            """
            INSERT INTO commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)  
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, f"newer-{days_ago}d-{int(commit_time.timestamp())}", commit_time)
        )
        recent_commits.append(commit_row["id"])
        
        # Create successful derivations for recent commits
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
                f"{hostname}-newer-build",
                f"/nix/store/newer-{days_ago}d-{hostname}.drv",
                commit_time + timedelta(minutes=10),
                commit_time + timedelta(minutes=25)
            )
        )
    
    # Update cleanup to include new commits
    cleanup_patterns = old_deployment["cleanup"].copy()
    cleanup_patterns["commits"].append(f"id IN ({','.join(map(str, recent_commits))})")
    cleanup_patterns["derivations"].append(f"derivation_name LIKE '{hostname}-newer-build'")
    
    return {**old_deployment, "recent_commit_ids": recent_commits, "cleanup": cleanup_patterns}


def scenario_flaky_agent(
    client: CFTestClient, hostname: str = "test-flaky-agent"
) -> Dict[str, Any]:
    """System with intermittent heartbeat gaps (unreliable network)"""
    
    now = datetime.now(UTC)
    
    # Create base system
    base = _create_base_scenario(
        client,
        hostname=hostname,
        flake_name="flaky-agent-test",
        repo_url="https://example.com/flaky-agent.git",
        git_hash="flaky-789",
        commit_age_hours=2,
        heartbeat_age_minutes=None,  # Custom heartbeat pattern
        system_ip="192.168.1.170"
    )
    
    system_state_id = base["state_id"]
    
    # Create intermittent heartbeat pattern over 24 hours
    # Pattern: normal, gap, recovery, gap, normal, gap, current
    heartbeat_pattern = [
        # Normal operation (24-12 hours ago)
        now - timedelta(hours=24), now - timedelta(hours=23), 
        now - timedelta(hours=22), now - timedelta(hours=21),
        now - timedelta(hours=20), now - timedelta(hours=19),
        
        # 6-hour gap (18-12 hours ago)
        
        # Brief recovery (12-10 hours ago) 
        now - timedelta(hours=12), now - timedelta(hours=11, minutes=30),
        
        # 2-hour gap (10-8 hours ago)
        
        # Normal operation (8-4 hours ago)
        now - timedelta(hours=8), now - timedelta(hours=7),
        now - timedelta(hours=6), now - timedelta(hours=5),
        
        # 1-hour gap (4-3 hours ago)
        
        # Recent recovery (last 3 hours)
        now - timedelta(hours=3), now - timedelta(hours=2, minutes=30),
        now - timedelta(hours=2), now - timedelta(hours=1, minutes=30),
        now - timedelta(hours=1), now - timedelta(minutes=30),
        now - timedelta(minutes=5)
    ]
    
    heartbeat_ids = []
    for i, hb_time in enumerate(heartbeat_pattern):
        hb_row = _one_row(
            client,
            """
            INSERT INTO agent_heartbeats (system_state_id, timestamp, agent_version, agent_build_hash)
            VALUES (%s, %s, '2.0.1', 'flaky-build')
            RETURNING id
            """,
            (system_state_id, hb_time)
        )
        heartbeat_ids.append(hb_row["id"])
    
    cleanup_patterns = base["cleanup"].copy()
    cleanup_patterns["agent_heartbeats"] = [f"id IN ({','.join(map(str, heartbeat_ids))})"]
    
    return {**base, "heartbeat_ids": heartbeat_ids}
