# Delivery setup uses real APIs, not a second Access assertion suite. Source
# and key creation precede registration; builders stay stopped until the source
# module records authoritative evaluation and verifies absence of its output.
admin = FACTS["admin"]
start_all()


def delivery_api(method, path, body=None, expected=200):
    status, response = http(controller, method, path, body,
                            session="delivery-admin", csrf=method != "GET")
    expect_equal(status, expected, method + " " + path)
    return response


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


with case("delivery-services-and-administrator-ready"):
    controller.wait_for_unit("crystal-forge-server.service", timeout=180)
    controller.wait_for_open_port(3000)
    services.wait_for_unit("nginx.service")
    services.wait_for_unit("sshd.service")
    for node in (controller, services, builderA, builderB, targetA):
        node.wait_for_unit("firewall.service")
        node.succeed("iptables -C OUTPUT -o eth0 -j REJECT && "
                     "ip6tables -C OUTPUT -o eth0 -j REJECT")
    receipt("delivery.publicEgressDenied", True)
    status, registered = http(controller, "POST", "/api/auth/local/register", {
        "username": admin["username"], "email": admin["email"],
        "password": admin["password"], "first_name": admin["firstName"],
        "last_name": admin["lastName"],
    })
    expect_equal(status, 201, "Delivery's own administrator registration")
    receipt("delivery.adminId", registered["user_id"])
    status, _ = http(controller, "POST", "/api/auth/local/login", {
        "username": admin["username"], "password": admin["password"],
    }, session="delivery-admin")
    expect_equal(status, 200, "Delivery administrator sign-in")
    who = delivery_api("GET", "/api/auth/whoami")
    expect_equal(who["roles"], ["Admin"], "setup actor role")
    environment = delivery_api("POST", "/api/v1/environments", {
        "name": "delivery-proof", "description": "Invocation-owned Delivery environment",
        "color_hex": "#2563eb", "is_active": True, "default_policy": "manual",
    }, expected=201)
    receipt("delivery.environmentId", environment["id"])

with case("source-authored-commit-served-by-local-git"):
    services.succeed(
        "cp -rL /etc/cf-it/source /tmp/delivery-source && chmod -R u+w /tmp/delivery-source && "
        "cat /proc/sys/kernel/random/uuid > /tmp/delivery-source/nonce && "
        "git init --bare /srv/git/delivery.git && "
        "git -C /tmp/delivery-source init -b main && "
        "git -C /tmp/delivery-source config user.name 'Delivery Author' && "
        "git -C /tmp/delivery-source config user.email 'delivery-author@example.test' && "
        "git -C /tmp/delivery-source add . && "
        "git -C /tmp/delivery-source commit -m 'Author nonce-bearing NixOS source' && "
        "git -C /tmp/delivery-source push /srv/git/delivery.git main && "
        "git --git-dir=/srv/git/delivery.git symbolic-ref HEAD refs/heads/main && "
        "git --git-dir=/srv/git/delivery.git update-server-info && "
        "chown -R git:git /srv/git/delivery.git"
    )
    source_commit = receipt("delivery.sourceRevision", services.succeed(
        "git -C /tmp/delivery-source rev-parse HEAD").strip())
    expect_equal(len(source_commit), 40, "full authored Git identity")
    source_nonce = receipt("delivery.sourceNonce", services.succeed(
        "cat /tmp/delivery-source/nonce").strip())
    receipt("delivery.sourceMessage", "Author nonce-bearing NixOS source")
    advertised = controller.succeed("git ls-remote " + shlex.quote(FACTS["flake"]["url"]) + " refs/heads/main")
    expect_equal(advertised.split()[0], source_commit, "real upstream advertised commit")

with case("api-builders-register-distinct-runtime-credentials"):
    ssh_public_keys = []
    signing_public_keys = []
    host_key = services.succeed("cat /etc/ssh/ssh_host_ed25519_key.pub").strip()
    receipt("delivery.cacheHostPublicKey", host_key)
    for node, name in ((builderA, "builderA"), (builderB, "builderB")):
        node.succeed(
            "install -d -m 0750 -o crystal-forge -g crystal-forge /var/lib/crystal-forge && "
            "cf-keygen -y -f /var/lib/crystal-forge/builder-api.key && "
            "chown crystal-forge:crystal-forge /var/lib/crystal-forge/builder-api.* "
            "&& chmod 600 /var/lib/crystal-forge/builder-api.key"
        )
        public_key = node.succeed("cat /var/lib/crystal-forge/builder-api.pub").strip()
        created = delivery_api("POST", "/api/v1/builders", {
            "name": name, "host": name, "arch": "x86_64-linux", "public_key": public_key,
            "max_cpu_cores": 2, "max_memory_mb": 2048, "max_concurrent_jobs": 1,
            "enabled": True, "environment_ids": [receipts()["delivery.environmentId"]],
        })
        expect_equal(created.get("private_key"), None, "server generates no replacement private key")
        receipt("delivery." + name + ".id", created["builder"]["id"])
        receipt("delivery." + name + ".publicKey", public_key)
        node.fail("systemctl is-active --quiet crystal-forge-builder.service")
        node.fail("systemctl is-active --quiet postgresql.service")
    expect(receipts()["delivery.builderA.id"] != receipts()["delivery.builderB.id"],
           "builders share a registered identity")
    expect(receipts()["delivery.builderA.publicKey"] != receipts()["delivery.builderB.publicKey"],
           "builders share a credential")
    # Both builder publication and the controller's normal background closure
    # publication need real credentials. They share only public trust material.
    for node, name in ((controller, "controller"), (builderA, "builderA"), (builderB, "builderB")):
        node.succeed("umask 077; ssh-keygen -q -t ed25519 -N '' -f /var/lib/crystal-forge/cache-ssh && "
                     "nix-store --generate-binary-cache-key " + shlex.quote("delivery-" + name) +
                     " /var/lib/crystal-forge/cache-sign.key /var/lib/crystal-forge/cache-sign.pub && "
                     "chown crystal-forge:crystal-forge /var/lib/crystal-forge/cache-ssh* "
                     "/var/lib/crystal-forge/cache-sign.* && "
                     "chmod 600 /var/lib/crystal-forge/cache-ssh /var/lib/crystal-forge/cache-sign.key")
        ssh_public_keys.append(node.succeed("cat /var/lib/crystal-forge/cache-ssh.pub").strip())
        signing_public_keys.append(node.succeed("cat /var/lib/crystal-forge/cache-sign.pub").strip())
        _vm_write(node, "/var/lib/crystal-forge/cache-known-hosts", "services " + host_key + "\n")
        node.succeed("chmod 644 /var/lib/crystal-forge/cache-known-hosts")
        # Scope environment allowlists need not carry NIX_SSHOPTS: the real
        # user's normal OpenSSH configuration pins identity and host trust.
        _vm_write(node, "/var/lib/crystal-forge/.ssh/config", "Host services\n"
                  "  User cache\n  IdentityFile /var/lib/crystal-forge/cache-ssh\n"
                  "  IdentitiesOnly yes\n  BatchMode yes\n  StrictHostKeyChecking yes\n"
                  "  UserKnownHostsFile /var/lib/crystal-forge/cache-known-hosts\n")
        node.succeed("chown -R crystal-forge:crystal-forge /var/lib/crystal-forge/.ssh && "
                     "chmod 700 /var/lib/crystal-forge/.ssh && chmod 600 /var/lib/crystal-forge/.ssh/config")
    _vm_write(services, "/var/lib/cache/.ssh/authorized_keys", "\n".join(ssh_public_keys) + "\n")
    services.succeed("chown -R cache:cache /var/lib/cache/.ssh && "
                     "chmod 700 /var/lib/cache/.ssh && chmod 600 /var/lib/cache/.ssh/authorized_keys")
    # Trust only public signing material from this run. Keep signature checks
    # enabled; an authenticated SSH connection alone does not authorize NARs.
    _vm_write(services, "/run/cf-it-cache-keys.conf",
              "\nextra-trusted-public-keys = " + " ".join(signing_public_keys) + "\n")
    services.succeed("cp /etc/nix/nix.conf /run/cf-it-nix.conf && "
                     "cat /run/cf-it-cache-keys.conf >> /run/cf-it-nix.conf && "
                     "rm /etc/nix/nix.conf && install -m 0644 /run/cf-it-nix.conf /etc/nix/nix.conf && "
                     "systemctl restart nix-daemon.service")
    for node in (controller, builderA, builderB):
        # Verify fixture credentials before attributing a publication failure
        # to the CF workflow. No private key leaves its originating VM.
        node.succeed("runuser -u crystal-forge -- ssh -o ConnectTimeout=5 cache@services true")
    # The target owns a separate read identity, never a builder's private key.
    # Its root-owned credentials survive live activation outside the agent's
    # transient RuntimeDirectory. Only public material reaches services.
    targetA.succeed("install -d -m 0700 /root/.ssh && "
                    "ssh-keygen -q -t ed25519 -N '' -f /root/.ssh/delivery-read")
    read_public_key = targetA.succeed("cat /root/.ssh/delivery-read.pub").strip()
    _vm_write(targetA, "/root/.ssh/known_hosts", "services " + host_key + "\n")
    _vm_write(targetA, "/root/.ssh/config", "Host services\n  User cache\n"
              "  IdentityFile /root/.ssh/delivery-read\n  IdentitiesOnly yes\n"
              "  BatchMode yes\n  StrictHostKeyChecking yes\n"
              "  UserKnownHostsFile /root/.ssh/known_hosts\n")
    targetA.succeed("chmod 600 /root/.ssh/config /root/.ssh/known_hosts")
    _vm_write(services, "/var/lib/cache/.ssh/authorized_keys", "\n".join(ssh_public_keys + [read_public_key]) + "\n")
    services.succeed("chown cache:cache /var/lib/cache/.ssh/authorized_keys && "
                     "chmod 600 /var/lib/cache/.ssh/authorized_keys")
    targetA.succeed("ssh -o ConnectTimeout=5 cache@services true")
    receipt("delivery.targetReadPublicKey", read_public_key)
    receipt("delivery.publisherPublicKeys", signing_public_keys)
    cache = delivery_api("POST", "/api/v1/caches", {
        "name": "delivery-local-store", "cache_type": "Nix", "push_to": FACTS["cacheUrl"],
        "enabled": True, "require_sigs": True,
        "max_retries": 0,
        "signing_key_path": "/var/lib/crystal-forge/cache-sign.key",
        "attic_public_key": " ".join(signing_public_keys),
        "environment_ids": [receipts()["delivery.environmentId"]],
    }, expected=201)
    receipt("delivery.cacheId", cache["id"])

with case("target-registers-and-reports-real-agent-state"):
    flake = delivery_api("POST", "/api/v1/flakes", {
        "name": FACTS["flake"]["name"], "repo_url": FACTS["flake"]["url"],
        "branch": FACTS["flake"]["branch"], "build_scope": "cf_systems_only",
    }, expected=201)
    receipt("delivery.flakeId", flake["id"])
    targetA.succeed("cf-keygen -y -f /run/cf-it-agent.key")
    target_key = targetA.succeed("cat /run/cf-it-agent.pub").strip()
    receipt("delivery.targetAgentPublicKey", target_key)
    target = delivery_api("POST", "/api/v1/systems", {
        "hostname": FACTS["target"], "system_configuration_name": FACTS["target"],
        "public_key": target_key, "environment": "delivery-proof",
        "flake_name": FACTS["flake"]["name"], "deployment_policy": "manual",
    }, expected=201)
    receipt("delivery.targetId", target["id"])
    delivery_api("PATCH", "/api/v1/systems/" + target["id"], {
        "hostname": FACTS["target"], "heartbeat_interval_secs": 15, "deployment_policy": "manual",
        "system_configuration_name": FACTS["target"], "environment": "delivery-proof",
        "flake_name": FACTS["flake"]["name"],
    })
    mapping = sql_json(controller, "SELECT flake_id, system_configuration_name FROM systems WHERE id="
                       + literal(target["id"]) + "::uuid")
    expect_equal(mapping, [{"flake_id": receipts()["delivery.flakeId"],
                            "system_configuration_name": FACTS["target"]}], "registered target/source mapping")
    targetA.succeed("systemctl start crystal-forge-agent.service")
    targetA.wait_for_unit("crystal-forge-agent.service")
    actual_running = targetA.succeed("readlink -f /run/current-system").strip()
    initial_profile_link = targetA.succeed("readlink /nix/var/nix/profiles/system || true").strip()
    receipt("delivery.initial", {
        "systemId": target["id"], "storePath": actual_running,
        "profileLink": initial_profile_link or None,
        "profile": targetA.succeed("test ! -e /nix/var/nix/profiles/system || readlink -f /nix/var/nix/profiles/system").strip() or None,
        "generation": int(initial_profile_link.split("-")[-2]) if initial_profile_link else None,
        "booted": targetA.succeed("readlink -f /run/booted-system").strip(),
        "bootId": targetA.succeed("cat /proc/sys/kernel/random/boot_id").strip(),
    })
    def target_report():
        rows = sql_json(controller, "SELECT s.id::text AS system_id, ss.id::text AS state_id, "
            "ss.store_path, ss.agent_version FROM systems s JOIN system_states ss "
            "ON ss.hostname=s.hostname WHERE s.id=" + literal(target["id"]) + "::uuid "
            "ORDER BY ss.timestamp DESC LIMIT 1")
        return rows if rows and rows[0]["store_path"] == actual_running else None
    report = wait_for(target_report, "real authenticated target state", timeout=120)[0]
    expect_equal(report["system_id"], target["id"], "reported registered target")
    expect(bool(report["agent_version"]), "agent reported no software identity")
    receipt("delivery.targetStateId", report["state_id"])
    receipt("delivery.targetRunningPath", actual_running)
    # Unchanged real-agent reports must reference S1 without inserting another
    # full state. A successful POST or a conservative DB-error fallback cannot
    # satisfy this assertion after the TASK-496 repair.
    def periodic_report():
        rows = sql_json(controller,
            "SELECT h.id::text AS id, h.system_state_id::text AS state_id, s.id::text AS system_id "
            "FROM agent_heartbeats h JOIN system_states ss ON ss.id=h.system_state_id "
            "JOIN systems s ON s.hostname=ss.hostname "
            "WHERE system_state_id=" + str(int(report["state_id"])) +
            " ORDER BY h.id")
        return rows if len(rows) >= 2 else None
    heartbeats = wait_for(periodic_report, "two real target heartbeats referencing S1", timeout=150)
    for heartbeat in heartbeats:
        expect_equal(heartbeat["system_id"], target["id"], "heartbeat registered system identity")
        expect_equal(heartbeat["state_id"], report["state_id"], "unchanged heartbeat state lineage")
    states = sql_json(controller, "SELECT id::text AS id, store_path FROM system_states WHERE hostname=" +
                      literal(FACTS["target"]) + " ORDER BY id")
    expect_equal(states, [{"id": report["state_id"], "store_path": actual_running}],
                 "unchanged target has exactly one full state")
    controller.fail("journalctl -u crystal-forge-server --no-pager | "
                    "grep -F 'Database error occurred while checking state'")
    receipt("delivery.targetHeartbeatId", heartbeats[0]["id"])
    receipt("delivery.targetHeartbeatIds", [h["id"] for h in heartbeats])
    receipt("delivery.targetHeartbeatRepresentation", "heartbeat")
