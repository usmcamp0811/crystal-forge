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
        flake_name: Name for the flake
        repo_url: Git repository URL
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

    # Insert flake
    flake_row = _one_row(
        client,
        """
        INSERT INTO public.flakes (name, repo_url)
        VALUES (%s, %s)
        ON CONFLICT (repo_url) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        """,
        (flake_name, repo_url),
    )
    flake_id = flake_row["id"]

    # Insert main commit
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
    scheduled_at = commit_ts + timedelta(minutes=5)
    completed_at = (
        commit_ts + timedelta(minutes=10) if derivation_status == "complete" else None
    )

    if derivation_status == "failed":
        completed_at = commit_ts + timedelta(minutes=10)
        drv_path = None  # Failed evaluations don't have derivation paths

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
    """System that has never been seen - only has flake and system entry"""
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

    # Create newer derivation and CAPTURE THE ID
    new_commit_id = result["additional_commit_ids"][0]
    now = datetime.now(UTC)
    new_scheduled = now - timedelta(minutes=50)
    new_completed = now - timedelta(minutes=45)

    new_deriv_row = _one_row(
        client,
        """
        INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, scheduled_at, completed_at
        )
        VALUES (%s, 'nixos', %s, %s, 
                (SELECT id FROM public.derivation_statuses WHERE name='complete'),
                0, %s, %s)
        RETURNING id
        """,
        (new_commit_id, hostname, new_drv, new_scheduled, new_completed),
    )
    new_deriv_id = new_deriv_row["id"]

    # ADD THE NEW DERIVATION TO CLEANUP PATTERNS
    result["cleanup"]["derivations"].append(f"id = {new_deriv_id}")
    result["new_derivation_id"] = new_deriv_id

    return result


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
        flake_name="failed-app",
        repo_url="https://example.com/failed.git",
        git_hash=old_hash,
        commit_age_hours=24,
        heartbeat_age_minutes=2,
        system_ip="192.168.1.103",
        additional_commits=[
            {"hash": new_hash, "age_hours": 0.5}  # Recent broken commit
        ],
    )

    # Create failed derivation for the new commit and CAPTURE THE ID
    new_commit_id = result["additional_commit_ids"][0]
    now = datetime.now(UTC)
    failed_completed = now - timedelta(minutes=30)

    failed_deriv_row = _one_row(
        client,
        """
        INSERT INTO public.derivations (
            commit_id, derivation_type, derivation_name, derivation_path,
            status_id, attempt_count, completed_at, error_message
        )
        VALUES (%s, 'nixos', %s, NULL,
                (SELECT id FROM public.derivation_statuses WHERE name='failed'),
                0, %s, 'Evaluation failed')
        RETURNING id
        """,
        (new_commit_id, hostname, failed_completed),
    )
    failed_deriv_id = failed_deriv_row["id"]

    # ADD THE FAILED DERIVATION TO CLEANUP PATTERNS
    result["cleanup"]["derivations"].append(f"id = {failed_deriv_id}")
    result["failed_derivation_id"] = failed_deriv_id

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
        heartbeat_age_minutes=45,  # Old heartbeat = offline
        system_ip="192.168.1.102",
    )


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
    """
    One flake, N commits over past `days`, M systems, heartbeats every interval, upgrades
    staggered within ~stagger_window_minutes of each commit. Tweak parameters above to adjust scale.
    Returns cleanup patterns.
    """
    from math import floor

    now = datetime.now(UTC)

    # Ensure flake exists; get flake_id
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

    # Status id for completed derivations
    status_rows = client.execute_sql(
        "SELECT id FROM public.derivation_statuses WHERE name='complete'"
    )
    complete_status_id = status_rows[0]["id"]

    # Create system hostnames (stable) and insert/ensure rows
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

    # Build commits spread across past `days`
    commit_hashes: List[str] = []
    derivation_paths_by_commit: List[str] = []

    total_hours = max(days, 1) * 24
    step = total_hours / max(num_commits - 1, 1)

    for idx in range(num_commits):
        age_hours = int(round(total_hours - idx * step))
        # Keep most recent commit at least 1 hour old
        age_hours = max(age_hours, 1)
        commit_ts = now - timedelta(hours=age_hours)
        git_hash = f"{flake_name}-c{idx+1:02d}-{int(commit_ts.timestamp())}"
        commit_hashes.append(git_hash)

        # Insert commit
        cr = client.execute_sql(
            """
            INSERT INTO public.commits (flake_id, git_commit_hash, commit_timestamp, attempt_count)
            VALUES (%s, %s, %s, 0)
            RETURNING id
            """,
            (flake_id, git_hash, commit_ts),
        )
        commit_id = cr[0]["id"]

        # Insert derivation for this commit
        drv_path = f"/nix/store/{git_hash[:12]}-nixos-system-{flake_name}.drv"
        derivation_paths_by_commit.append(drv_path)
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

        # For each commit, every system "upgrades" within ~stagger_window_minutes
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

        # Keep systems table pointing at "current" derivation as we progress
        client.execute_sql(
            """
            UPDATE public.systems SET derivation = %s
            WHERE hostname = ANY(%s)
            """,
            (drv_path, hostnames),
        )

    # Heartbeats every heartbeat_interval over last heartbeat_hours, for latest state per system
    interval_minutes = heartbeat_interval_minutes
    beats_per_system = floor(heartbeat_hours * 60 / interval_minutes)

    for hn in hostnames:
        # Latest state id for the host
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

    # Cleanup patterns
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
