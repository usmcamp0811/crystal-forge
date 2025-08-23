from __future__ import annotations

from ..test_context import CrystalForgeTestContext
from .base import BaseViewTests

SQL_EXISTS = "systems_status_view_exists"


class SystemsStatusTableTests(BaseViewTests):
    """Test suite for view_systems_status_table"""

    @staticmethod
    def run_all_tests(ctx: CrystalForgeTestContext) -> None:
        ctx.logger.log_section("🔍 Testing view_systems_status_table")

        if not SystemsStatusTableTests._view_exists(ctx, SQL_EXISTS):
            ctx.logger.log_warning("View does not exist - skipping remaining tests")
            return

        # --- SQL-driven path (kept) ---
        SystemsStatusTableTests._test_status_logic_scenarios(ctx)
        SystemsStatusTableTests._test_heartbeat_system_state_interactions(ctx)
        SystemsStatusTableTests._test_update_status_logic(ctx)
        SystemsStatusTableTests._test_edge_cases(ctx)

        # --- Agent-driven first use example ---
        SystemsStatusTableTests._test_status_logic_via_agent(ctx)

        # Perf + cleanup (existing)
        SystemsStatusTableTests._test_view_performance(ctx)
        SystemsStatusTableTests.cleanup_test_data(ctx)

    # ---------------- Agent-driven example ----------------
    @staticmethod
    def _test_status_logic_via_agent(ctx: CrystalForgeTestContext) -> None:
        runner_cmd = "/run/current-system/sw/bin/test-agent"  # adjust if different
        sudo_user = "crystalforge"  # run as service user that talks to server API

        agent_cfg = {
            "scenario": "system_status",
            "hostname": "test-online",
            "os": "25.05",
            "kernel": "6.6.89",
            "memory_gb": 16,
            "cpu": {"brand": "Test CPU", "cores": 8},
            "ip": "10.0.0.202",
            "nixos_version": "25.05",
            "agent": {"compatible": True, "version": "1.2.3", "build": "abc123def"},
            "state_at": {"minutes_ago": 5},
            "heartbeat_at": {"minutes_ago": 2},
        }

        query_sql = """
            SELECT hostname, connectivity_status, connectivity_status_text
            FROM view_systems_status_table
            WHERE hostname = 'test-online';
        """.strip()

        def assert_online(rows):
            if not rows:
                return "No rows returned for 'test-online'"
            row = rows[0]
            if len(row) < 3:
                return f"Expected ≥3 columns, got {row}"
            host, status, text = row[0], row[1], row[2]
            if host != "test-online":
                return f"hostname mismatch: {host}"
            if status != "online":
                return f"status mismatch: expected online, got {status}"
            if not text:
                return "connectivity_status_text is empty"
            return None

        BaseViewTests._agent_scenario(
            ctx,
            runner_cmd=runner_cmd,
            sudo_user=sudo_user,
            agent_config=agent_cfg,
            query_sql=query_sql,
            test_name="Agent-driven: recent heartbeat + state => online",
            assert_fn=assert_online,
            config_arg="--config",
            extra_args=["--once"],
            wait_after_secs=1,
        )
