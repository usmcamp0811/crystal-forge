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

# When run directly, add parent directory to path so cf_test_modules imports resolve
if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).parent.parent))

from cf_test_modules import (  # :contentReference[oaicite:3]{index=3}
    CrystalForgeTestContext,
    DatabaseTests,
)
from cf_test_modules.devshell_adapter import (  # :contentReference[oaicite:4]{index=4}
    DevShellLogger,
    create_devshell_server_vm,
)
from cf_test_modules.test_exceptions import (
    AssertionFailedException,
)  # :contentReference[oaicite:5]{index=5}


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
    system_info = {"hostname": hostname}

    return CrystalForgeTestContext(
        gitserver=None,
        server=server_vm,
        agent=None,
        logger=logger,
        system_info=system_info,
        exit_on_failure=True,
    )


def create_ctx_for_nixos() -> CrystalForgeTestContext:
    """
    Bind directly to the NixOS test-driver Machine objects already created
    by the test script (server/gitserver/agent if present).
    """
    logger = DevShellLogger()  # Reuse simple console logger formatting
    server = _find_machine_in_callstack("server")
    if server is None:
        raise RuntimeError("Could not locate 'server' Machine in test-driver context")

    # Optional peers if present in the test script
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
        system_info={"hostname": hostname},
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
    logger.log_section(
        "🚀 Crystal Forge Database Tests - "
        + (
            "DevShell Mode"
            if isinstance(ctx.server.__class__.__name__, str)
            and ctx.system_info.get("hostname", "").startswith("devshell")
            else "VM Mode"
        )
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
    args = parser.parse_args()

    mode = detect_mode(args.mode)
    logger = DevShellLogger()

    if mode == "devshell":
        if not check_prereqs_devshell(logger):
            sys.exit(1)
        ctx = create_ctx_for_devshell()
    else:
        # nixos-test-driver path
        ctx = create_ctx_for_nixos()

    run_database_tests(ctx)


if __name__ == "__main__":
    main()
