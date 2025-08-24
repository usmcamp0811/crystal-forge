#!/usr/bin/env python3
"""
Crystal Forge Unified Test Runner

- Auto-detects NixOS VM test-driver vs. devshell (process-compose) environments.
- Can be forced via --mode {auto,devshell,nixos} or CF_TEST_MODE={devshell,nixos}.
- In devshell mode it uses the DevShellVM/Logger adapter and localhost DB params.
- In NixOS VM mode it binds directly to the real test-driver Machine objects.
"""

import argparse
import inspect
import os
import sys
from pathlib import Path
from typing import Callable, Dict, List, Tuple

# When run directly, add parent directory to path so cf_test_modules imports resolve
if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).parent.parent))

from cf_test_modules import (  # moved: re-exported from package root
    AgentTests, CrystalForgeServerTests, CrystalForgeTestContext,
    DatabaseTests, FlakeProcessingTests, SystemStateTests)
from cf_test_modules.exceptions.test_exceptions import \
    AssertionFailedException  # moved under exceptions/; :contentReference[oaicite:5]{index=5}
from cf_test_modules.runtime.devshell_adapter import (  # moved: re-exported from package root
    DevShellLogger, create_devshell_server_vm)

# --------------------------------------------------------------------------------------
# --- phase registry & groups ---
# --------------------------------------------------------------------------------------
PHASES: List[Tuple[str, Callable, str]] = [
    ("Phase DB.1: Database Setup", DatabaseTests.setup_and_verify, "db.setup"),
    ("Phase DB.2: Database View Tests", DatabaseTests.run_view_tests, "db.views"),
    (
        "Phase 2.1: Crystal Forge Server Tests",
        CrystalForgeServerTests.setup_and_verify,
        "server",
    ),
    ("Phase 2.2: Agent Tests", AgentTests.setup_and_verify, "agent"),
    (
        "Phase 3.1: Flake Processing Tests",
        FlakeProcessingTests.verify_complete_workflow,
        "flake",
    ),
    (
        "Phase 3.2: System State Tests",
        SystemStateTests.verify_system_state_tracking,
        "system",
    ),
]
ALL_TAGS: List[str] = [t for _, _, t in PHASES]
TAG_TO_PHASE: Dict[str, Tuple[str, Callable]] = {t: (n, f) for (n, f, t) in PHASES}

GROUPS: Dict[str, List[str]] = {
    "db": ["db.setup", "db.views"],
    "core": ["server", "agent", "flake", "system"],
    "all": ALL_TAGS,
}


def _resolve_phase_spec(spec: str | None) -> List[Tuple[str, Callable]]:
    if not spec:
        spec = "db"
    tokens = [s.strip().lower() for s in spec.split(",") if s.strip()]
    ordered: List[Tuple[str, Callable]] = []
    seen: set[str] = set()
    for tok in tokens:
        keys = GROUPS.get(tok, [tok])
        for key in keys:
            if key in TAG_TO_PHASE and key not in seen:
                ordered.append(TAG_TO_PHASE[key])
                seen.add(key)
    return ordered


def _log_suite_header(ctx: CrystalForgeTestContext) -> None:
    mode = (ctx.system_info or {}).get("mode", "").lower()
    if mode not in {"devshell", "nixos", "vm"}:
        mode = (
            "nixos"
            if (_try_import_test_driver() and _find_machine_in_callstack("server"))
            else "devshell"
        )
    ctx.logger.log_section(
        f"🚀 Crystal Forge Tests - {'VM Mode' if mode in {'nixos','vm'} else 'DevShell Mode'}"
    )


def run_selected_phases(ctx: CrystalForgeTestContext, spec: str | None) -> None:
    logger = ctx.logger
    _log_suite_header(ctx)
    try:
        for name, fn in _resolve_phase_spec(spec):
            run_phase(logger, name, fn, ctx)
        logger.log_success("🎉 Selected phases completed!")
    finally:
        logger.log_section("📋 Test execution completed")


# --------------------------------------------------------------------------------------
# Environment detection
# --------------------------------------------------------------------------------------
def _try_import_test_driver():
    try:
        import test_driver  # type: ignore

        return test_driver
    except Exception:
        return None


def _find_machine_in_callstack(name: str):
    """
    Walk the call stack to find a variable named `name` that looks like a
    nixos-test-driver Machine object (best-effort; avoids changing test script).
    """
    td = _try_import_test_driver()
    MachineT = None
    if td is not None:
        # Try common locations
        try:
            from test_driver.machine import Machine as MachineT  # type: ignore
        except Exception:
            MachineT = None

    for frame_info in inspect.stack():
        locs = frame_info.frame.f_locals
        if name in locs:
            candidate = locs[name]
            if MachineT is None:
                # If we can't import the class, still accept anything matching API
                if hasattr(candidate, "succeed") and hasattr(
                    candidate, "wait_for_unit"
                ):
                    return candidate
            else:
                if isinstance(candidate, MachineT):
                    return candidate
    return None


def detect_mode(cli_mode: str | None) -> str:
    """
    Decide between 'devshell' and 'nixos'.
    Priority: CLI arg -> CF_TEST_MODE env -> auto-detect.
    """
    # Normalize
    if cli_mode:
        if cli_mode.lower() in {"nixos", "vm"}:
            return "nixos"
        if cli_mode.lower() == "devshell":
            return "devshell"
        # auto -> fall through
    env_mode = os.getenv("CF_TEST_MODE", "").lower()
    if env_mode in {"nixos", "vm"}:
        return "nixos"
    if env_mode == "devshell":
        return "devshell"

    # Auto-detect: presence of test_driver AND a visible Machine (server)
    if (
        _try_import_test_driver() is not None
        and _find_machine_in_callstack("server") is not None
    ):
        return "nixos"

    return "devshell"


# --------------------------------------------------------------------------------------
# Context creation
# --------------------------------------------------------------------------------------
def create_ctx_for_devshell() -> CrystalForgeTestContext:
    logger = DevShellLogger()
    server_vm = create_devshell_server_vm()
    hostname = os.getenv("HOSTNAME", "devshell-test")

    return CrystalForgeTestContext(
        gitserver=None,
        server=server_vm,
        agent=None,
        logger=logger,
        system_info={"hostname": hostname, "mode": "devshell"},
        exit_on_failure=True,
    )


def create_ctx_for_nixos() -> CrystalForgeTestContext:
    logger = DevShellLogger()  # reuse simple console logger
    server = _find_machine_in_callstack("server")
    if server is None:
        raise RuntimeError("Could not locate 'server' Machine in test-driver context")

    gitserver = _find_machine_in_callstack("gitserver")
    agent = _find_machine_in_callstack("agent")

    try:
        hostname = server.succeed("hostname -s").strip()
    except Exception:
        hostname = "nixos-vm"

    return CrystalForgeTestContext(
        gitserver=gitserver,
        server=server,
        agent=agent,
        logger=logger,
        system_info={"hostname": hostname, "mode": "nixos"},
        exit_on_failure=True,
    )


# --------------------------------------------------------------------------------------
# Test phases
# --------------------------------------------------------------------------------------
def run_phase(logger: DevShellLogger, name: str, func, *args, **kwargs):
    logger.log_section(f"🚀 STARTING: {name}")
    try:
        func(*args, **kwargs)
        logger.log_success(f"✅ COMPLETED: {name}")
    except AssertionFailedException as e:
        logger.log_error(f"❌ ASSERTION FAILED: {name}")
        logger.log_error(f"🔍 {e}")
        raise
    except Exception as e:
        import traceback

        tb = traceback.extract_tb(e.__traceback__)
        if tb:
            last = tb[-1]
            loc = f"{Path(last.filename).name}::{last.name}() line {last.lineno}"
        else:
            loc = "unknown"
        logger.log_error(f"❌ FAILED: {name}")
        logger.log_error(f"🔍 {loc}")
        logger.log_error(f"🔍 {e}")
        raise


def check_prereqs_devshell(logger: DevShellLogger) -> bool:
    """
    Only for devshell: verify local postgres is reachable (process-compose).
    """
    db_host = os.getenv("DB_HOST", "127.0.0.1")
    db_port = os.getenv("DB_PORT", "3042")

    try:
        import subprocess

        r = subprocess.run(
            ["pg_isready", "-h", db_host, "-p", db_port],
            capture_output=True,
            timeout=5,
        )
        if r.returncode != 0:
            logger.log_error(f"PostgreSQL not ready on {db_host}:{db_port}")
            logger.log_error(
                "Ensure process-compose is running (e.g., `nix run .#cf-dev`)."
            )
            return False
        logger.log_success(f"PostgreSQL ready on {db_host}:{db_port}")
    except (FileNotFoundError, Exception):
        logger.log_warning(
            "pg_isready unavailable; skipping explicit DB readiness probe."
        )

    return True


def run_database_tests(ctx: CrystalForgeTestContext) -> None:
    logger = ctx.logger
    mode = (ctx.system_info or {}).get("mode", "").lower()
    if mode not in {"devshell", "nixos", "vm"}:
        # Fallback if caller forgot to set mode
        mode = (
            "nixos"
            if (_try_import_test_driver() and _find_machine_in_callstack("server"))
            else "devshell"
        )

    logger.log_section(
        f"🚀 Crystal Forge Database Tests - {'VM Mode' if mode in {'nixos','vm'} else 'DevShell Mode'}"
    )

    try:
        run_phase(
            logger, "Phase DB.1: Database Setup", DatabaseTests.setup_and_verify, ctx
        )
        run_phase(
            logger, "Phase DB.2: Database View Tests", DatabaseTests.run_view_tests, ctx
        )
        logger.log_success("🎉 Database-related tests passed!")
    finally:
        logger.log_section("📋 Test execution completed")


# --------------------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------------------
def main():
    parser = argparse.ArgumentParser(description="Crystal Forge Unified Test Runner")
    parser.add_argument(
        "--mode",
        choices=["auto", "devshell", "nixos", "vm"],
        default="auto",
        help="Execution mode selection",
    )
    parser.add_argument(
        "--phases",
        default="db",
        help="Comma-separated phase tags or groups "
        "(db, core, all, server, agent, flake, system, db.setup, db.views)",
    )
    args = parser.parse_args()

    mode = detect_mode(args.mode)
    logger = DevShellLogger()

    if mode == "devshell":
        if not check_prereqs_devshell(logger):
            sys.exit(1)
        ctx = create_ctx_for_devshell()
    else:
        ctx = create_ctx_for_nixos()

    run_selected_phases(ctx, args.phases)


if __name__ == "__main__":
    main()
