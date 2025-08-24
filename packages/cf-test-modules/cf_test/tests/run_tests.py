#!/usr/bin/env python3
"""
Crystal Forge Systems Status View Test Runner
"""
import os
import subprocess
import sys
from pathlib import Path


def main():
    """Run systems status view tests"""

    # Set up environment
    test_env = os.environ.copy()
    test_env.setdefault("DB_HOST", "127.0.0.1")
    test_env.setdefault("DB_PORT", "5432")
    test_env.setdefault("DB_USER", "crystal_forge")
    test_env.setdefault("DB_PASSWORD", "password")
    test_env.setdefault("DB_NAME", "crystal_forge")

    # Determine test directory
    test_dir = Path(__file__).parent
    output_dir = test_dir / "test-results"
    output_dir.mkdir(exist_ok=True)

    print("🧪 Crystal Forge Systems Status View Tests")
    print("=" * 50)
    print(
        f"Database: {test_env['DB_USER']}@{test_env['DB_HOST']}:{test_env['DB_PORT']}/{test_env['DB_NAME']}"
    )
    print(f"Output: {output_dir}")
    print("")

    # Parse command line arguments
    if len(sys.argv) > 1:
        test_filter = sys.argv[1]
    else:
        test_filter = None

    # Build pytest command
    cmd = [
        sys.executable,
        "-m",
        "pytest",
        "--tb=short",
        "--maxfail=5",
        "-v",
        f"--junit-xml={output_dir}/junit.xml",
        f"--html={output_dir}/report.html",
        "--self-contained-html",
    ]

    # Add test filters
    if test_filter == "smoke":
        cmd.extend(["-m", "smoke"])
        print("🚀 Running smoke tests only...")
    elif test_filter == "systems-status":
        cmd.extend(["-m", "systems_status"])
        print("🖥️  Running systems status tests...")
    elif test_filter == "views":
        cmd.extend(["-m", "views"])
        print("👁️  Running view tests...")
    elif test_filter == "quick":
        cmd.extend(["-m", "smoke or (views and not slow)"])
        print("⚡ Running quick tests...")
    else:
        print("🏃 Running all tests...")
        if test_filter:
            cmd.extend(["-k", test_filter])

    # Add test directory
    cmd.append(str(test_dir))

    print(f"Command: {' '.join(cmd[2:])}")  # Skip python -m pytest
    print("")

    # Run tests
    try:
        result = subprocess.run(cmd, env=test_env, cwd=test_dir)

        print("")
        print("📊 Test Results:")
        print(f"  JUnit XML: {output_dir}/junit.xml")
        print(f"  HTML Report: {output_dir}/report.html")

        if result.returncode == 0:
            print("✅ All tests passed!")
        else:
            print(f"❌ Tests failed (exit code: {result.returncode})")

        return result.returncode

    except KeyboardInterrupt:
        print("\n⏹️  Tests interrupted by user")
        return 130
    except Exception as e:
        print(f"❌ Error running tests: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
