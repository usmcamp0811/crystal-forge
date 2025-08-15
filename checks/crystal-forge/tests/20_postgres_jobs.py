# timer configured
server.succeed("systemctl list-timers | grep crystal-forge-postgres-jobs")

# run once
server.succeed("systemctl start crystal-forge-postgres-jobs.service")
server.succeed(
    "journalctl -u crystal-forge-postgres-jobs.service | grep 'All jobs completed successfully'"
)

# idempotent
server.succeed("systemctl start crystal-forge-postgres-jobs.service")
server.succeed(
    "journalctl -u crystal-forge-postgres-jobs.service | tail -20 | grep 'All jobs completed successfully'"
)

server.log("=== postgres jobs validation completed ===")
