from ..runtime.test_context import CrystalForgeTestContext

class SystemStateTests:
    """System state tracking tests"""

    @staticmethod
    def verify_system_state_tracking(ctx: CrystalForgeTestContext) -> None:
        """Verify system state tracking functionality"""
        SystemStateTests._verify_agent_system_states(ctx)
        SystemStateTests._verify_heartbeat_functionality(ctx)

    @staticmethod
    def _verify_agent_system_states(ctx: CrystalForgeTestContext) -> None:
        """Verify agent system state recording"""
        ctx.logger.log_section("📊 Verifying System State Tracking")

        # Count total system states
        systems_count = ctx.server.succeed(
            """psql -U crystal_forge -d crystal_forge -c 'SELECT COUNT(*) FROM system_states;' -t"""
        ).strip()
        ctx.logger.log_info(f"Total system states recorded: {systems_count}")

        # Verify agent system state
        agent_states_count = ctx.server.succeed(
            f"""psql -U crystal_forge -d crystal_forge -c "SELECT COUNT(*) FROM system_states WHERE hostname = '{ctx.system_info['hostname']}';" -t"""
        ).strip()
        ctx.logger.log_info(
            f"System states for agent '{ctx.system_info['hostname']}': {agent_states_count}"
        )

        if int(agent_states_count) > 0:
            ctx.logger.log_success(
                f"Agent '{ctx.system_info['hostname']}' system states recorded successfully"
            )
        else:
            ctx.logger.log_warning(
                f"No system states found for agent '{ctx.system_info['hostname']}'"
            )

    @staticmethod
    def _verify_heartbeat_functionality(ctx: CrystalForgeTestContext) -> None:
        """Verify heartbeat functionality"""
        # This could include testing periodic heartbeats
        # For now, we just verify the initial connection worked
        pass
