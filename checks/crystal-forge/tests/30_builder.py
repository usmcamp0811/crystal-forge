# enable & start
server.succeed("systemctl enable crystal-forge-builder.service")
server.succeed("systemctl start crystal-forge-builder.service")
server.wait_for_unit("crystal-forge-builder.service")
server.succeed("systemctl is-active crystal-forge-builder.service")

# nix available
server.succeed("sudo -u crystal-forge nix --version")

# dirs and ownership
server.succeed("test -d /var/lib/crystal-forge/workdir")
server.succeed("stat -c '%U' /var/lib/crystal-forge/workdir | grep -q crystal-forge")
server.succeed("test -d /var/lib/crystal-forge/.cache/nix")
server.succeed("stat -c '%U' /var/lib/crystal-forge/.cache/nix | grep -q crystal-forge")

# logs
server.wait_until_succeeds(
    "journalctl -u crystal-forge-builder.service | grep 'Starting Build loop'",
    timeout=30,
)

# memory reasonable (<4GiB) and limits set
memory_usage = server.succeed(
    "systemctl show crystal-forge-builder.service --property=MemoryCurrent"
)
server.log(f"Builder memory usage: {memory_usage}")
if "MemoryCurrent=" in memory_usage:
    mem_bytes = int(memory_usage.split("=")[1].strip() or "0")
    assert (
        mem_bytes <= 4 * 1024 * 1024 * 1024
    ), f"Builder using excessive memory: {mem_bytes}"

server.succeed(
    "systemctl show crystal-forge-builder.service --property=MemoryMax | grep -v infinity"
)
server.succeed(
    "systemctl show crystal-forge-builder.service --property=TasksMax | grep -v infinity"
)

# reload/restart
server.succeed("systemctl reload-or-restart crystal-forge-builder.service")
server.wait_for_unit("crystal-forge-builder.service")
server.wait_until_succeeds(
    "journalctl -u crystal-forge-builder.service | grep 'Starting Build loop'",
    timeout=30,
)

# cleanup
server.succeed("sudo -u crystal-forge touch /var/lib/crystal-forge/workdir/result-test")
server.succeed(
    "sudo -u crystal-forge ln -sf /nix/store/fake /var/lib/crystal-forge/workdir/result-old"
)
server.succeed("systemctl restart crystal-forge-builder.service")
server.wait_for_unit("crystal-forge-builder.service")
server.wait_until_fails("test -L /var/lib/crystal-forge/workdir/result-old", timeout=30)

# coexistence
server_status = server.succeed("systemctl is-active crystal-forge-server.service")
builder_status = server.succeed("systemctl is-active crystal-forge-builder.service")
assert (
    "active" in server_status and "active" in builder_status
), "Server and builder services are conflicting"
