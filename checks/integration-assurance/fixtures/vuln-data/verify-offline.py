"""Checks real vulnix matching with locally parsed upstream feeds, without servers.

This is a cheap scanner/parser check, not CF, lease, HTTPS or browser evidence.
The cache contains real NVD objects parsed by the pinned scanner, never expected
scanner JSON. The subprocess runs the packaged CLI against real Nix outputs.
"""

import argparse
from datetime import datetime
import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile

from vulnix.nvd import Archive, NVD


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scanner", required=True)
    parser.add_argument("--feed-root", required=True, type=Path)
    parser.add_argument("--fault-feed-root", required=True, type=Path)
    parser.add_argument("--terminal-fault-feed-root", type=Path)
    parser.add_argument("--vulnerable", required=True)
    parser.add_argument("--corrected", required=True)
    args = parser.parse_args()
    facts = json.loads(Path(__file__).with_name("facts.json").read_text())
    version = subprocess.check_output([args.scanner, "--version"], text=True).strip()
    assert version == facts["scannerVersion"], version
    feeds = args.feed_root / "feeds/json/cve/2.0"
    for path in sorted(feeds.glob("*.gz")):
        archive = Archive(path.name)
        archive.parse(gzip.decompress(path.read_bytes()).decode())
        expected = [facts["cveId"]] if any(
            suffix in path.name for suffix in ("2026", "modified", "recent")
        ) else []
        assert list(archive.advisories) == expected, path
    for path in sorted((args.fault_feed_root / "feeds/json/cve/2.0").glob("*.gz")):
        try:
            Archive(path.name).parse(gzip.decompress(path.read_bytes()).decode())
        except json.JSONDecodeError:
            pass
        else:
            raise AssertionError(f"fault archive unexpectedly parsed: {path}")

    check_cli(args, facts, feeds, facts["cvssV3"])
    if args.terminal_fault_feed_root:
        terminal_feeds = args.terminal_fault_feed_root / "feeds/json/cve/2.0"
        for path in sorted(terminal_feeds.glob("*.gz")):
            archive = Archive(path.name)
            archive.parse(gzip.decompress(path.read_bytes()).decode())
            assert list(archive.advisories) == [facts["cveId"]], path
            assert archive.advisories[facts["cveId"]].cvssv3 == 11, path
        check_cli(args, facts, terminal_feeds, 11)
    print("PASS: real pinned CLI identities, vulnerable score and corrected clean control")


def check_cli(args, facts, feeds, score):
    """Runs real CLI matching using a temporary cache of real parsed NVD inputs."""
    with tempfile.TemporaryDirectory(prefix="cf-assurance-offline-") as tmp:
        cache = str(Path(tmp) / "nvd")
        with NVD(cache_dir=cache) as nvd:
            archive = Archive(2026)
            archive.parse(gzip.decompress((feeds / "nvdcve-2.0-2026.json.gz").read_bytes()).decode())
            nvd.add(archive)
            nvd.reindex()
            # Deliberately warm the local parser check. A cold real download
            # and upstream-fault exit status require the coordinator's service.
            nvd.meta.last_update = datetime.now()
        env = dict(os.environ, HOME=tmp, NIX_CONFIG="substituters =\n")
        for output, expected_version, expected_exit in (
            (args.vulnerable, facts["vulnerableVersion"], 2),
            (args.corrected, facts["correctedVersion"], 0),
        ):
            subprocess.run(["nix-store", "--check-validity", output], check=True, env=env)
            drv = subprocess.check_output(
                ["nix-store", "--query", "--deriver", output], text=True, env=env
            ).strip()
            assert drv.endswith(".drv"), drv
            command = [args.scanner, "--json", output, "--cache-dir", cache,
                       "--mirror", "http://127.0.0.1:1/no-network-allowed/"]
            result = subprocess.run(command, text=True, capture_output=True, env=env, timeout=60)
            print(json.dumps({"command": command, "exit": result.returncode,
                              "stdout": result.stdout, "stderr": result.stderr}), flush=True)
            assert result.returncode == expected_exit, result.stderr
            entries = json.loads(result.stdout)
            if expected_exit == 0:
                assert entries == [], entries
                continue
            assert len(entries) == 1, entries
            entry = entries[0]
            for field, expected in {
                "name": facts["package"] + "-" + expected_version,
                "pname": facts["package"], "version": expected_version,
                "derivation": drv, "affected_by": [facts["cveId"]],
                "whitelisted": [], "cvssv3_basescore": {facts["cveId"]: score},
            }.items():
                assert entry[field] == expected, (field, entry)


if __name__ == "__main__":
    main()
