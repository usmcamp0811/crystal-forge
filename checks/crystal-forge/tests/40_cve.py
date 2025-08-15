# optional vulnix tests
vulnix_available = (
    server.succeed(
        "bash -lc 'command -v vulnix >/dev/null && echo yes || echo no'"
    ).strip()
    == "yes"
)

if vulnix_available:
    server.wait_until_succeeds(
        "journalctl -u crystal-forge-builder.service | grep 'Starting CVE Scan loop'",
        timeout=30,
    )
    # allow some time for scan to appear in logs
    server.wait_until_succeeds(
        "journalctl -u crystal-forge-builder.service | grep -E '(CVE scan|vulnix)'",
        timeout=120,
    )
else:
    server.log("Warning: vulnix not available for CVE scanning tests")
