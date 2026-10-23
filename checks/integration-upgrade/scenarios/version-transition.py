# D2 approved exact-release transition. All writes use real CF workflows.
# SQL is observational only. The installed PostgreSQL cluster, identities and
# target persist across normal NixOS package/configuration specialisations.
admin = FACTS["admin"]
start_all()
current_server = False


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


def api(method, path, body=None, expected=200):
    status, response = http(controller, method, path, body,
                            session="upgrade-admin", csrf=method != "GET")
    expect_equal(status, expected, method + " " + path)
    return response


def executable(node, unit, expected):
    path = node.succeed("readlink -f /proc/$(systemctl show -p MainPID --value " +
                        shlex.quote(unit) + ")/exe").strip()
    if path != expected:
        directory, program = expected.rsplit("/", 1)
        wrapped = directory + "/." + program + "-wrapped"
        expect_equal(path, wrapped, node.name + " actual " + unit + " executable")
        # Production makeWrapper outputs execute this exact companion binary.
        # Check the pinned wrapper binding, not merely a common store prefix.
        node.succeed("grep -F -- " + shlex.quote('exec -a "$0" "' + wrapped + '"') +
                     " " + shlex.quote(expected) + " >/dev/null")
    return path


def upgrade(node):
    node.succeed("/run/current-system/specialisation/current/bin/switch-to-configuration test", timeout=180)


def agent_credential_metadata(phase):
    """Records ownership and configured paths without reading credential contents."""
    unit = targetA.succeed("systemctl show crystal-forge-agent --no-pager "
        "-p User -p Group -p MainPID -p ExecStart -p WorkingDirectory "
        "-p StateDirectory -p RuntimeDirectory -p ProtectSystem -p ProtectHome")
    process = targetA.succeed("pid=$(systemctl show -p MainPID --value crystal-forge-agent); "
        "ps -p \"$pid\" -o pid=,euid=,egid=,user=,group=")
    configured = targetA.succeed("awk '/^\\[client\\]/{client=1;next} /^\\[/{client=0} "
        "client && /^private_key[[:space:]]*=/{print;found=1} END{if(!found)exit 1}' "
        "/var/lib/crystal-forge-agent/config.toml")
    files = targetA.succeed("stat -Lc '%n|%F|%a|%u|%g|%s' "
        "/var/lib/upgrade-identity /var/lib/upgrade-identity/agent.key "
        "/var/lib/upgrade-identity/agent.pub /var/lib/crystal-forge-agent "
        "/var/lib/crystal-forge-agent/config.toml /run/crystal-forge-agent "
        "/root/.ssh /root/.ssh/upgrade-read /root/.ssh/config")
    ssh_identity = targetA.succeed("awk '$1 == \"IdentityFile\" {print}' /root/.ssh/config")
    receipt("upgrade." + phase + ".credentialMetadata", {
        "unit": unit, "process": process, "clientConfig": configured, "files": files,
        "sshIdentityConfigSource": "/root/.ssh/config", "sshIdentity": ssh_identity,
        "predecessorModuleSource": FACTS["predecessor"]["source"] + "/modules/nixos/crystal-forge/default.nix",
    })


def author(phase):
    if phase == "current":
        services.succeed("touch /tmp/upgrade-source/current-agent && git -C /tmp/upgrade-source add current-agent")
    services.succeed("printf '%s-' " + shlex.quote(phase) + " > /tmp/upgrade-source/nonce && "
        "cat /proc/sys/kernel/random/uuid >> /tmp/upgrade-source/nonce && "
        "git -C /tmp/upgrade-source add nonce && git -C /tmp/upgrade-source commit -m " +
        shlex.quote("Upgrade " + phase) + " && git -C /tmp/upgrade-source push "
        "--receive-pack='runuser -u git -- git-receive-pack' /srv/git/upgrade.git main")
    sha = services.succeed("git -C /tmp/upgrade-source rev-parse HEAD").strip()
    nonce = services.succeed("cat /tmp/upgrade-source/nonce").strip()
    api("POST", "/api/v1/flakes/" + str(receipts()["upgrade.flakeId"]) + "/sync", {})
    def evaluated():
        rows = sql_json(controller, "SELECT c.id AS commit_id, c.git_commit_hash, "
            "d.id AS derivation_id, d.derivation_path, d.store_path, j.id::text AS job_id, j.status "
            "FROM commits c JOIN derivations d ON d.commit_id=c.id "
            "JOIN build_jobs j ON j.derivation_id=d.id WHERE c.git_commit_hash=" + literal(sha) +
            " AND d.derivation_name='targetA' ORDER BY j.created_at")
        return rows[0] if rows and rows[0]["derivation_path"] else None
    row = wait_for(evaluated, phase + " actual release/current evaluation and queue admission", timeout=300)
    output = controller.succeed("nix-store --query --outputs " + shlex.quote(row["derivation_path"])).strip()
    artifact = receipt("upgrade." + phase, dict(row, commit=sha, nonce=nonce, outputPath=output))
    if not current_server:
        # This release's native worker requires the evaluated recipe locally;
        # it has no server-derivation transport. Stage only prerequisite recipes
        # and source objects through the existing local store, never outputs.
        for node in (controller, services, builderA):
            assert_store_absent(node, output)
        # Do not inherit root's profile PATH into this unprivileged SSH client.
        command = ("runuser -u crystal-forge -- env HOME=/var/lib/crystal-forge "
                   "PATH=/run/current-system/sw/bin nix copy --derivation ")
        controller.succeed(command + "--to ssh-ng://cache@services " + shlex.quote(row["derivation_path"]), timeout=180)
        builderA.succeed(command + "--from ssh-ng://cache@services " + shlex.quote(row["derivation_path"]), timeout=180)
        for node in (services, builderA):
            assert_store_absent(node, output)
        receipt("upgrade." + phase + ".predecessorRecipeStaged", row["derivation_path"])
    return artifact


def terminal(artifact):
    last_rows = []
    def finished():
        nonlocal last_rows
        rows = sql_json(controller, "SELECT id::text, derivation_id, status, builder_id::text, logs "
            "FROM build_jobs WHERE derivation_id=" + str(artifact["derivation_id"]) + " ORDER BY created_at")
        last_rows = rows
        return rows if rows and any(r["status"] == "success" for r in rows) else None
    try:
        rows = wait_for(finished, "actual authoritative build success for " + artifact["commit"], timeout=600)
    except Exception:
        receipt("upgrade.failedBuild." + artifact["commit"], last_rows)
        raise
    expect_equal(sum(r["status"] == "success" for r in rows), 1, "one authoritative success across physical attempts")
    expect(not any(r["status"] in ("building", "assigned", "queued", "cancelling") for r in rows),
           "success coexists with active contradictory attempt")
    return rows


def deploy(artifact, phase):
    import uuid
    request = api("POST", "/api/v1/systems/" + receipts()["upgrade.targetId"] + "/deploy", {
        "commit_sha": artifact["commit"], "action": "deploy", "request_id": str(uuid.uuid4()),
    }, expected=202)
    receipt("upgrade." + phase + ".request", request)
    if phase == "compatible":
        # A normal old-service restart polls immediately through its genuine
        # signed heartbeat path; it does not change the installed agent package.
        targetA.succeed("systemctl restart crystal-forge-agent")
        executable(targetA, "crystal-forge-agent", FACTS["predecessor"]["agent"])
        agent_credential_metadata(phase + "BeforeDeployment")
    # The exact release hard-codes a 600-second heartbeat cadence. This bound
    # covers its next ordinary poll plus activation, not a compatibility expiry.
    activation_timeout = 720 if phase == "compatible" else 300
    targetA.wait_until_succeeds("test \"$(readlink -f /run/current-system)\" = " +
                              shlex.quote(artifact["outputPath"]), timeout=activation_timeout)
    expect_equal(targetA.succeed("cat /etc/cf-it-revision").strip(), artifact["nonce"], "actual activated source marker")
    def reported():
        rows = sql_json(controller, "SELECT id::text, store_path, agent_version, generation FROM system_states "
            "WHERE hostname='targetA' ORDER BY id DESC LIMIT 1")
        return rows[0] if rows and rows[0]["store_path"] == artifact["outputPath"] else None
    report = wait_for(reported, phase + " real signed agent report after activation", timeout=180)
    receipt("upgrade." + phase + ".report", report)
    def succeeded():
        rows = sql_json(controller, "SELECT id::text, system_id::text, target_store_path, status "
            "FROM pending_system_deployments WHERE id=" + literal(request["deployment_id"]) + "::uuid")
        return rows if rows and rows[0]["status"] == "succeeded" else None
    rows = wait_for(succeeded, phase + " durable correlated deployment success", timeout=180)
    expect_equal(rows[0]["system_id"], receipts()["upgrade.targetId"], "same registered SystemID")
    expect_equal(rows[0]["target_store_path"], artifact["outputPath"], "actual result binds intended output")
    events = sql_json(controller, "SELECT id::text FROM system_events WHERE deployment_id=" +
        literal(request["deployment_id"]) + "::uuid AND event_type='cf_deployment_succeeded'")
    expect_equal(len(events), 1, "one authoritative deployment success event")
    return report


with case("exact-predecessor-services-and-source-ready"):
    controller.wait_for_unit("crystal-forge-server.service", timeout=180)
    controller.wait_for_open_port(3000, timeout=180)
    services.wait_for_unit("nginx.service")
    services.wait_for_unit("sshd.service")
    receipt("upgrade.predecessor", FACTS["predecessor"])
    receipt("upgrade.oldServerExecutable", executable(controller, "crystal-forge-server", FACTS["predecessor"]["server"]))
    for node in (controller, services, builderA, builderB, targetA):
        node.wait_for_unit("firewall.service")
        node.succeed("iptables -C OUTPUT -o eth0 -j REJECT && ip6tables -C OUTPUT -o eth0 -j REJECT")
    status, registered = http(controller, "POST", "/api/auth/local/register", {
        "username": admin["username"], "email": admin["email"], "password": admin["password"],
        "first_name": admin["firstName"], "last_name": admin["lastName"],
    })
    expect_equal(status, 201, "real predecessor creates local password credential")
    receipt("upgrade.adminId", registered["user_id"])
    status, _ = http(controller, "POST", "/api/auth/local/login", {
        "username": admin["username"], "password": admin["password"],
    }, session="upgrade-admin")
    expect_equal(status, 200, "predecessor authenticates its created credential")
    env = api("POST", "/api/v1/environments", {"name": "upgrade-proof", "description": "Disposable D2 transition",
        "color_hex": "#2563eb", "is_active": True, "default_policy": "manual"}, expected=201)
    receipt("upgrade.environmentId", env["id"])
    services.succeed("cp -rL /etc/cf-it/source /tmp/upgrade-source && chmod -R u+w /tmp/upgrade-source && "
        "cat /proc/sys/kernel/random/uuid > /tmp/upgrade-source/nonce && "
        "git init --bare /srv/git/upgrade.git && git -C /tmp/upgrade-source init -b main && "
        "git -C /tmp/upgrade-source config user.name 'Upgrade Author' && "
        "git -C /tmp/upgrade-source config user.email upgrade@example.test && "
        "git -C /tmp/upgrade-source add . && git -C /tmp/upgrade-source commit -m 'Initial source' && "
        "git -C /tmp/upgrade-source push /srv/git/upgrade.git main && "
        "git --git-dir=/srv/git/upgrade.git symbolic-ref HEAD refs/heads/main && chown -R git:git /srv/git/upgrade.git")

with case("predecessor-registers-real-builder-cache-and-agent-credentials"):
    publisher_keys = []
    ssh_keys = []
    host_key = services.succeed("cat /etc/ssh/ssh_host_ed25519_key.pub").strip()
    for node in (controller, builderA, builderB):
        # Operator-managed credentials live outside the application's recursive
        # tmpfiles permission rule. Their 0600 modes must survive activation.
        node.succeed("install -d -m 0750 -o crystal-forge -g crystal-forge /var/lib/crystal-forge "
            "/var/lib/upgrade-credentials && umask 077 && "
            "ssh-keygen -q -t ed25519 -N '' -f /var/lib/upgrade-credentials/cache-ssh && "
            "nix-store --generate-binary-cache-key upgrade-" + node.name +
            " /var/lib/upgrade-credentials/cache-sign.key /var/lib/upgrade-credentials/cache-sign.pub && "
            "chown crystal-forge:crystal-forge /var/lib/upgrade-credentials/cache-*")
        ssh_keys.append(node.succeed("cat /var/lib/upgrade-credentials/cache-ssh.pub").strip())
        publisher_keys.append(node.succeed("cat /var/lib/upgrade-credentials/cache-sign.pub").strip())
        _vm_write(node, "/var/lib/crystal-forge/.ssh/known_hosts", "services " + host_key + "\n")
        _vm_write(node, "/var/lib/crystal-forge/.ssh/config", "Host services\n  User cache\n"
            "  IdentityFile /var/lib/upgrade-credentials/cache-ssh\n  IdentitiesOnly yes\n"
            "  BatchMode yes\n  StrictHostKeyChecking yes\n"
            "  UserKnownHostsFile /var/lib/crystal-forge/.ssh/known_hosts\n")
        node.succeed("chown -R crystal-forge:crystal-forge /var/lib/crystal-forge/.ssh && "
                      "chmod 700 /var/lib/crystal-forge/.ssh && chmod 600 /var/lib/crystal-forge/.ssh/config")
        receipt("upgrade.predecessorCacheCredentials." + node.name, {
            "account": node.succeed("id -u crystal-forge && id -g crystal-forge").strip(),
            "unitAccount": node.succeed("systemctl show " +
                ("crystal-forge-server" if node == controller else "crystal-forge-builder") +
                " --no-pager -p User -p Group").strip(),
            "files": node.succeed("stat -Lc '%n|%F|%a|%u|%g|%s' /var/lib/upgrade-credentials "
                "/var/lib/upgrade-credentials/cache-ssh /var/lib/upgrade-credentials/cache-sign.key").strip(),
            "sshIdentitySource": "/var/lib/crystal-forge/.ssh/config",
            "sshIdentity": node.succeed("awk '$1 == \"IdentityFile\" {print}' /var/lib/crystal-forge/.ssh/config").strip(),
            "signingKeySource": "old module cache.signing_key; cache API signing_key_path",
        })
        if node == controller:
            receipt("upgrade.predecessorServerProcessIdentity", node.succeed(
                "pid=$(systemctl show -p MainPID --value crystal-forge-server); "
                "ps -p \"$pid\" -o pid=,euid=,egid=,user=,group=").strip())
    for node in (builderA, builderB):
        node.succeed("cf-keygen -y -f /var/lib/crystal-forge/builder-api.key && "
            "chown crystal-forge:crystal-forge /var/lib/crystal-forge/builder-api.* && "
            "chmod 600 /var/lib/crystal-forge/builder-api.key")
        key = node.succeed("cat /var/lib/crystal-forge/builder-api.pub").strip()
        created = api("POST", "/api/v1/builders", {"name": node.name, "host": node.name, "arch": "x86_64-linux",
            "public_key": key, "max_cpu_cores": 2, "max_memory_mb": 2048, "max_concurrent_jobs": 1,
            "enabled": True, "environment_ids": [env["id"]]})
        receipt("upgrade." + node.name + ".id", created["builder"]["id"])
        receipt("upgrade." + node.name + ".publicKey", key)
        _vm_write(node, "/var/lib/crystal-forge/release-builder.env",
            "CRYSTAL_FORGE__BUILDER__ENABLE_API_MODE=true\nCRYSTAL_FORGE__BUILDER__BUILDER_ID=" +
            created["builder"]["id"] + "\n")
        node.succeed("chmod 600 /var/lib/crystal-forge/release-builder.env")
    targetA.succeed("install -d -m 0700 /var/lib/upgrade-identity /root/.ssh && "
        "cf-keygen -y -f /var/lib/upgrade-identity/agent.key && chmod 600 /var/lib/upgrade-identity/agent.key && "
        "ssh-keygen -q -t ed25519 -N '' -f /root/.ssh/upgrade-read")
    ssh_keys.append(targetA.succeed("cat /root/.ssh/upgrade-read.pub").strip())
    _vm_write(targetA, "/root/.ssh/known_hosts", "services " + host_key + "\n")
    _vm_write(targetA, "/root/.ssh/config", "Host services\n  User cache\n  IdentityFile /root/.ssh/upgrade-read\n"
        "  IdentitiesOnly yes\n  StrictHostKeyChecking yes\n  BatchMode yes\n")
    targetA.succeed("chmod 600 /root/.ssh/config")
    _vm_write(services, "/var/lib/cache/.ssh/authorized_keys", "\n".join(ssh_keys) + "\n")
    services.succeed("chown -R cache:cache /var/lib/cache/.ssh && chmod 700 /var/lib/cache/.ssh && "
        "chmod 600 /var/lib/cache/.ssh/authorized_keys")
    for node in (services, targetA):
        _vm_write(node, "/run/upgrade-nix-trust", "\nextra-trusted-public-keys = " + " ".join(publisher_keys) + "\n")
        node.succeed("cp /etc/nix/nix.conf /run/upgrade-nix.conf && cat /run/upgrade-nix-trust >> /run/upgrade-nix.conf && "
            "rm /etc/nix/nix.conf && install -m 0644 /run/upgrade-nix.conf /etc/nix/nix.conf && systemctl restart nix-daemon")
    cache = api("POST", "/api/v1/caches", {"name": "upgrade-local-store", "cache_type": "Nix",
        "push_to": "ssh-ng://cache@services", "enabled": True, "require_sigs": True,
        "signing_key_path": "/var/lib/upgrade-credentials/cache-sign.key", "attic_public_key": " ".join(publisher_keys),
        "environment_ids": [env["id"]], "max_retries": 0}, expected=201)
    receipt("upgrade.cacheId", cache["id"])
    flake = api("POST", "/api/v1/flakes", {"name": FACTS["flake"]["name"], "repo_url": FACTS["flake"]["url"],
        "branch": "main", "build_scope": "cf_systems_only"}, expected=201)
    receipt("upgrade.flakeId", flake["id"])
    agent_key = targetA.succeed("cat /var/lib/upgrade-identity/agent.pub").strip()
    receipt("upgrade.agentPublicKey", agent_key)
    target = api("POST", "/api/v1/systems", {"hostname": "targetA", "system_configuration_name": "targetA",
        "public_key": agent_key, "environment": "upgrade-proof", "flake_name": FACTS["flake"]["name"],
        "deployment_policy": "manual"}, expected=201)
    receipt("upgrade.targetId", target["id"])
    api("PATCH", "/api/v1/systems/" + target["id"], {"hostname": "targetA",
        "deployment_policy": "manual", "system_configuration_name": "targetA", "environment": "upgrade-proof",
        "flake_name": FACTS["flake"]["name"]})
    targetA.succeed("systemctl start crystal-forge-agent")
    executable(targetA, "crystal-forge-agent", FACTS["predecessor"]["agent"])
    wait_for(lambda: sql_json(controller, "SELECT id FROM system_states WHERE hostname='targetA'"),
             "real predecessor signed state", timeout=180)
    agent_credential_metadata("predecessorStartup")

# Registration and a real signed state make migration/compatibility probes safe
# even if the release cannot complete a build. Keep those mandatory failures;
# never substitute prepared results for the failed release activity.
failures = []
try:
    with case("predecessor-completes-native-build-signed-publication-and-observed-history"):
        first = author("completed")
        for node in (controller, services, builderA, builderB, targetA):
            assert_store_absent(node, first["outputPath"])
        builderA.succeed("systemctl start crystal-forge-builder")
        builderA.wait_for_unit("crystal-forge-builder")
        receipt("upgrade.oldBuilderExecutable", executable(builderA, "crystal-forge-builder", FACTS["predecessor"]["builder"]))
        receipt("upgrade.predecessorBuilderProcessIdentity", builderA.succeed(
            "pid=$(systemctl show -p MainPID --value crystal-forge-builder); "
            "ps -p \"$pid\" -o pid=,euid=,egid=,user=,group=").strip())
        terminal(first)
        services.wait_until_succeeds("nix-store --check-validity " + shlex.quote(first["outputPath"]), timeout=180)
        expect_equal(services.succeed("cat " + shlex.quote(first["outputPath"] + "/cf-it-source-nonce")).strip(),
                     first["nonce"], "published output contains the runtime-authored source nonce")
        services.succeed("nix store verify --no-contents --sigs-needed 1 " + shlex.quote(first["outputPath"]), timeout=180)
        def published():
            rows = sql_json(controller, "SELECT id, derivation_id, status, store_path, cache_destination, completed_at "
                "FROM cache_push_jobs WHERE derivation_id=" + str(first["derivation_id"]) + " ORDER BY id")
            return rows if rows and all(row["status"] == "completed" and row["completed_at"] for row in rows) else None
        publications = wait_for(published, "real predecessor completed publication records", timeout=180)
        expect(all(row["store_path"] == first["outputPath"] and row["cache_destination"] == "ssh-ng://cache@services"
                   for row in publications), "release publication lost its exact output/destination relationship")
        receipt("upgrade.completedPublications", publications)
        # Installed-state observation is independent of the completed build.
        # D2 requires old-agent activation with the CURRENT server below; no
        # old-server manual deployment or invented success record is needed.
        installed = targetA.succeed("readlink -f /run/current-system").strip()
        expect(installed != first["outputPath"], "producer output was unexpectedly activated")
        history = sql_json(controller, "SELECT id::text, hostname, timestamp, store_path, generation, agent_version, change_reason "
            "FROM system_states WHERE hostname='targetA' ORDER BY id")
        expect(bool(history) and history[-1]["store_path"] == installed, "signed history does not describe the actual installed state")
        receipt("upgrade.completedHistory", history)
        heartbeats = sql_json(controller, "SELECT h.id::text, h.system_state_id::text FROM agent_heartbeats h "
            "JOIN system_states ss ON ss.id=h.system_state_id WHERE ss.hostname='targetA' ORDER BY h.id")
        # The release stores its authenticated initial report as a full state,
        # not an agent_heartbeats row. Preserve any later heartbeat links without
        # requiring the ten-minute periodic poll in this producer phase. The
        # old-agent/current-server heartbeat remains mandatory after cutover.
        expect(all(any(state["id"] == beat["system_state_id"] for state in history)
                   for beat in heartbeats), "authenticated heartbeat lost its observed-state relationship")
        receipt("upgrade.predecessorHeartbeats", heartbeats)
        audit = sql_json(controller, "SELECT id::text, actor_user_id::text, action, target, metadata, created_at "
            "FROM admin_audit_events WHERE actor_user_id=" + literal(receipts()["upgrade.adminId"]) + "::uuid ORDER BY id")
        expect(bool(audit), "predecessor workflow created no audit attribution")
        receipt("upgrade.predecessorAudit", audit)
        receipt("upgrade.completedJobs", sql_json(controller, "SELECT id::text, derivation_id, status, builder_id::text "
            "FROM build_jobs WHERE derivation_id=" + str(first["derivation_id"]) + " ORDER BY id"))
except Exception as error:
    failures.append(str(error))

try:
    with case("predecessor-leaves-actual-interrupted-claim-and-distinct-queued-work"):
        try:
            builderA.succeed("systemctl stop crystal-forge-builder crystal-forge-builds.slice", timeout=180)
            interrupted = author("interrupted")
            builderA.succeed("systemctl start crystal-forge-builder")
            def claimed():
                rows = sql_json(controller, "SELECT id::text, derivation_id, status, builder_id::text FROM build_jobs "
                    "WHERE id=" + literal(interrupted["job_id"]) + "::uuid")
                return rows[0] if rows and rows[0]["status"] == "building" else None
            claim = wait_for(claimed, "genuine release builder claim in flight", timeout=180)
            receipt("upgrade.interruptedClaim", claim)
            wait_for(lambda: sql_json(controller, "SELECT id FROM build_jobs WHERE id=" +
                literal(claim["id"]) + "::uuid AND logs LIKE '%upgrade-interrupted-build-started%'"),
                "actual interrupted Nix execution before stopping its owner", timeout=180)
        finally:
            # Stop the actual owner even when its required execution proof
            # failed. Distinct queued input and snapshots remain real data.
            builderA.succeed("systemctl stop crystal-forge-builder crystal-forge-builds.slice", timeout=180)
            receipt("upgrade.oldMigrations", sql_json(controller, "SELECT version, encode(checksum,'hex') AS checksum "
                "FROM _sqlx_migrations WHERE success ORDER BY version"))
            receipt("upgrade.databaseIdentity", sql_json(controller, "SELECT system_identifier::text FROM pg_control_system()"))
            receipt("upgrade.identity", sql_json(controller, "SELECT id::text, hostname, public_key, flake_id "
                "FROM systems WHERE id=" + literal(target["id"]) + "::uuid"))
            queued = author("queued")
            expect_equal(queued["status"], "queued", "distinct unclaimed release queue entry")
            if "upgrade.interrupted" in receipts():
                expect(interrupted["job_id"] != queued["job_id"], "queued and interrupted logical identities collapsed")
except Exception as error:
    failures.append(str(error))

with case("controlled-maintenance-upgrades-stopped-builders-before-server"):
    controller.succeed("systemctl stop crystal-forge-postgres-jobs.timer crystal-forge-postgres-jobs.service "
        "crystal-forge-server.service")
    for node in (builderA, builderB):
        node.succeed("systemctl stop crystal-forge-builder crystal-forge-builds.slice")
        node.fail("systemctl is-active --quiet crystal-forge-builds.slice")
        upgrade(node)
        node.fail("systemctl is-active --quiet crystal-forge-builder")
        # Current preStart is deliberately held until server readiness. Remove
        # the release-generated credential-bearing config before the cutover.
        node.succeed("rm -f /var/lib/crystal-forge/config.toml /var/lib/crystal-forge/release-builder.env")
        node.succeed("iptables -I OUTPUT -d $(getent ahostsv4 controller | awk 'NR==1{print $1}') "
                     "-p tcp --dport 5432 -j REJECT && "
                     "ip6tables -I OUTPUT -p tcp --dport 5432 -j REJECT")
    # Firewall closes the former release-only DB transport, including existing
    # connections. This is independent of normal generated HBA config changes.
    controller.succeed("iptables -I INPUT -p tcp --dport 5432 ! -s 127.0.0.1 -j REJECT && "
                       "ip6tables -I INPUT -p tcp --dport 5432 ! -s ::1 -j REJECT")
    expect_equal(sql_json(controller, "SELECT count(*)::int AS n FROM pg_stat_activity WHERE "
        "datname='crystal_forge' AND client_addr IS NOT NULL AND client_addr <> '127.0.0.1'::inet"), [{"n": 0}],
        "no release builder DB connections survive maintenance")
    # Prevent activation from starting builders; normal current server startup
    # owns SQLx migration of the same existing cluster.
    upgrade(controller)
    controller.wait_for_unit("crystal-forge-server", timeout=180)
    controller.wait_for_open_port(3000, timeout=180)
    receipt("upgrade.currentServerExecutable", executable(controller, "crystal-forge-server", FACTS["current"]["server"]))
    current_server = True
    expect_equal(api("GET", "/api/auth/whoami")["user"]["id"], receipts()["upgrade.adminId"],
                 "real predecessor session credential remains usable")
    expect_equal(sql_json(controller, "SELECT system_identifier::text FROM pg_control_system()"),
                 receipts()["upgrade.databaseIdentity"], "same installed PostgreSQL cluster")
    migrations = sql_json(controller, "SELECT version, encode(checksum,'hex') AS checksum "
        "FROM _sqlx_migrations WHERE success ORDER BY version")
    expect(all(row in migrations for row in receipts()["upgrade.oldMigrations"]), "release migration checksums changed")
    expect(len(migrations) > len(receipts()["upgrade.oldMigrations"]), "no normal current migrations observed")
    receipt("upgrade.currentMigrations", migrations)
    # The predecessor API has no heartbeat-interval field. Configure the
    # current server through its real supported PATCH contract (minimum 15s),
    # rather than assuming an ignored old-version field changed negotiation.
    api("PATCH", "/api/v1/systems/" + target["id"], {"hostname": "targetA", "heartbeat_interval_secs": 15,
        "deployment_policy": "manual", "system_configuration_name": "targetA", "environment": "upgrade-proof",
        "flake_name": FACTS["flake"]["name"]})
    expect_equal(sql_json(controller, "SELECT heartbeat_interval_secs FROM systems WHERE id=" +
        literal(target["id"]) + "::uuid"), [{"heartbeat_interval_secs": 15}], "real current heartbeat negotiation configuration")
    receipt("upgrade.currentHeartbeatIntervalSecs", 15)
    for node in (controller, builderA, builderB):
        node.succeed("test \"$(stat -c %a /var/lib/upgrade-credentials/cache-ssh)\" = 600 && "
                     "test \"$(stat -c %a /var/lib/upgrade-credentials/cache-sign.key)\" = 600")
    for node in (builderA, builderB):
        node.fail("systemctl is-active --quiet crystal-forge-builder")
        node.succeed("systemctl start crystal-forge-builder")
        node.wait_for_unit("crystal-forge-builder", timeout=120)
        executable(node, "crystal-forge-builder", FACTS["current"]["builder"])
        node.succeed("! grep -q '^\\[database\\]' /var/lib/crystal-forge/config.toml")
        node.fail("systemctl is-active --quiet postgresql")

# Migration succeeded, so independent recovery and rollout probes can continue
# safely after a semantic failure. Every failed case remains failed in evidence;
# the final assertion keeps the aggregate red. No failure becomes an xfail/skip.
try:
    with case("release-identities-credentials-and-completed-history-survive"):
        expect_equal(sql_json(controller, "SELECT id::text, hostname, public_key, flake_id FROM systems WHERE id=" +
            literal(target["id"]) + "::uuid"), receipts()["upgrade.identity"], "registered identity and signing key")
        status, _ = http(controller, "POST", "/api/auth/local/login", {"username": admin["username"],
            "password": admin["password"]}, session="upgrade-password")
        expect_equal(status, 200, "persisted predecessor password verifies on current server")
        expect(all(key in receipts() for key in ("upgrade.completedHistory", "upgrade.completedJobs", "upgrade.completedPublications",
                                               "upgrade.predecessorHeartbeats", "upgrade.predecessorAudit")),
               "completed-history proof is blocked: real predecessor build/publication or signed observation evidence is missing")
        current_events = sql_json(controller, "SELECT id::text, hostname, timestamp, store_path, generation, agent_version, change_reason "
            "FROM system_states WHERE hostname='targetA' ORDER BY id")
        expect(all(row in current_events for row in receipts()["upgrade.completedHistory"]), "completed historical evidence changed")
        current_audit = sql_json(controller, "SELECT id::text, actor_user_id::text, action, target, metadata, created_at "
            "FROM admin_audit_events WHERE actor_user_id=" + literal(receipts()["upgrade.adminId"]) + "::uuid ORDER BY id")
        expect(all(row in current_audit for row in receipts()["upgrade.predecessorAudit"]), "predecessor audit identities or attribution changed")
        expect_equal(sql_json(controller, "SELECT id::text, derivation_id, status, builder_id::text FROM build_jobs "
            "WHERE derivation_id=" + str(first["derivation_id"]) + " ORDER BY id"), receipts()["upgrade.completedJobs"],
            "completed release job identity/result/claim attribution")
        expect_equal(sql_json(controller, "SELECT id, derivation_id, status, store_path, cache_destination, completed_at "
            "FROM cache_push_jobs WHERE derivation_id=" + str(first["derivation_id"]) + " ORDER BY id"),
            receipts()["upgrade.completedPublications"], "completed release publication identity/output/destination relationship")
        heartbeats = sql_json(controller, "SELECT h.id::text, h.system_state_id::text FROM agent_heartbeats h "
            "JOIN system_states ss ON ss.id=h.system_state_id WHERE ss.hostname='targetA' ORDER BY h.id")
        expect(all(row in heartbeats for row in receipts()["upgrade.predecessorHeartbeats"]),
               "predecessor authenticated heartbeat/state relationships changed")
except Exception as error:
    failures.append(str(error))

try:
    with case("interrupted-release-claim-is-fenced-after-current-ownership"):
        expect("upgrade.interruptedClaim" in receipts(), "stale-claim proof is blocked: no actual predecessor claim was observed")
        claim = receipts()["upgrade.interruptedClaim"]
        def replacement():
            rows = sql_json(controller, "SELECT current_session_id::text AS session FROM builders WHERE id=" +
                literal(receipts()["upgrade.builderA.id"]) + "::uuid")
            return rows[0] if rows and rows[0]["session"] else None
        receipt("upgrade.replacementSession", wait_for(replacement, "real current replacement builder session", timeout=180))
        # Hold legitimate current writers during the before/after observation;
        # otherwise their progress could be mistaken for a stale-probe effect.
        with unit_stopped(builderA, "crystal-forge-builder"), unit_stopped(builderB, "crystal-forge-builder"):
            before = sql_json(controller, "SELECT to_jsonb(j) AS job FROM build_jobs j WHERE id=" + literal(claim["id"]) + "::uuid")
            path = "/api/v1/builders/" + receipts()["upgrade.builderA.id"] + "/jobs/" + claim["id"] + "/complete"
            _vm_write(builderA, "/run/upgrade-stale-body", json.dumps({"output_path": interrupted["outputPath"]}))
            builderA.succeed("node /etc/cf-it/signed-request.cjs " + shlex.join([
                receipts()["upgrade.builderA.id"], path, "/run/upgrade-stale-body"]))
            status = int(builderA.succeed("curl -sS -o /run/upgrade-stale-response -w '%{http_code}' "
                "-X POST -H @/run/upgrade-probe-headers --data-binary @/run/upgrade-stale-body " +
                shlex.quote("http://controller:3000" + path)).strip())
            expect(status in (401, 403, 409), "obsolete actual release claim can commit completion: HTTP " + str(status))
            expect_equal(sql_json(controller, "SELECT to_jsonb(j) AS job FROM build_jobs j WHERE id=" +
                literal(claim["id"]) + "::uuid"), before, "stale signed completion has no persistence effect")
            receipt("upgrade.staleClaimRejected", status)
except Exception as error:
    failures.append(str(error))

try:
    with case("queued-and-interrupted-release-operations-recover-authoritatively"):
        for phase, artifact in (("interrupted", interrupted), ("queued", queued)):
            # Current admission requires a current authoritative evaluation.
            # Use the existing operator action for the SAME exact commit; do
            # not replace old jobs or fabricate a current evaluation artifact.
            receipt("upgrade." + phase + ".beforeRecovery", sql_json(controller,
                "SELECT id::text, derivation_id, status, server_failure_code FROM build_jobs WHERE derivation_id=" +
                str(artifact["derivation_id"]) + " ORDER BY created_at"))
            previous_attempt = sql_json(controller, "SELECT evaluation_attempt_count AS n FROM commits WHERE id=" +
                str(artifact["commit_id"]))[0]["n"]
            api("POST", "/api/v1/commits/" + str(artifact["commit_id"]) + "/re-evaluate", {})
            def current_evaluation():
                return sql_json(controller, "SELECT a.id::text, a.attempt_number FROM evaluation_attempts a "
                    "JOIN evaluation_attempt_inventory i ON i.evaluation_attempt_id=a.id WHERE a.commit_id=" +
                    str(artifact["commit_id"]) + " AND a.status='complete' AND i.execution_mode='native' "
                    "AND a.attempt_number > " + str(previous_attempt) + " ORDER BY a.attempt_number DESC LIMIT 1")
            receipt("upgrade." + phase + ".recoveryEvaluation", wait_for(current_evaluation,
                "exact-commit current native reevaluation before retry", timeout=300))
            source_job = sql_json(controller, "SELECT status FROM build_jobs WHERE id=" + literal(artifact["job_id"]) + "::uuid")
            if source_job and source_job[0]["status"] in ("failed", "cancelled"):
                receipt("upgrade." + phase + ".retryRequest", api("POST", "/api/v1/build-jobs/" +
                    artifact["job_id"] + "/requeue", {}))
            attempts = terminal(artifact)
            receipt("upgrade." + phase + ".recoveredAttempts", attempts)
            original = sql_json(controller, "SELECT id::text, derivation_id FROM build_jobs WHERE id=" +
                literal(artifact["job_id"]) + "::uuid")
            expect_equal(original, [{"id": artifact["job_id"], "derivation_id": artifact["derivation_id"]}],
                         "original logical operation and audit identity retained")
            lineage = sql_json(controller, "SELECT id::text, automatic_retry_source_id::text, "
                "parent_job_id::text, root_job_id::text, attempt_number, status FROM build_jobs WHERE derivation_id=" +
                str(artifact["derivation_id"]) + " ORDER BY created_at")
            ids = {a["id"] for a in lineage}
            for attempt in lineage:
                if attempt["id"] != artifact["job_id"]:
                    expect(attempt["automatic_retry_source_id"] in ids or attempt["parent_job_id"] in ids,
                           "successor attempt lost its original audit relationship")
                    expect_equal(attempt["root_job_id"], artifact["job_id"], "successor retains logical root identity")
            receipt("upgrade." + phase + ".lineage", lineage)
            services.succeed("nix-store --check-validity " + shlex.quote(artifact["outputPath"]))
except Exception as error:
    failures.append(str(error))

heartbeat_proved = False
try:
    with case("exact-release-agent-signed-heartbeat-and-state-on-current-server"):
        old_states = sql_json(controller, "SELECT max(id) AS id FROM agent_heartbeats")
        executable(targetA, "crystal-forge-agent", FACTS["predecessor"]["agent"])
        # The release POSTs a signed full SystemState with startup context to
        # /agent/heartbeat. Current storage can classify it as liveness; that
        # does not prove a thin heartbeat message. The periodic loop is fixed
        # at ten minutes and ignores the current server's interval setting.
        targetA.succeed("systemctl restart crystal-forge-agent")
        def heartbeat():
            rows = sql_json(controller, "SELECT h.id::text, h.system_state_id::text, ss.agent_version, ss.store_path "
                "FROM agent_heartbeats h JOIN system_states ss ON ss.id=h.system_state_id WHERE ss.hostname='targetA' "
                "AND h.id > " + str(old_states[0]["id"] or 0) + " ORDER BY h.id")
            return rows if rows else None
        receipt("upgrade.oldAgentCurrentHeartbeat", wait_for(heartbeat, "exact release signed heartbeat accepted by current server", timeout=180))
        receipt("upgrade.oldAgentCurrentProtocol", {
            "endpoint": "/agent/heartbeat", "payload": "signed full SystemState",
            "trigger": "agent restart; startup context", "thinHeartbeatObserved": False,
            "source": FACTS["predecessor"]["source"] + "/packages/default/src/bin/agent.rs",
            "journal": targetA.succeed("journalctl -u crystal-forge-agent -b --no-pager -o cat "
                "--grep='\\[startup\\] Current System:|Posting heartbeat to:'").strip(),
        })
        executable(targetA, "crystal-forge-agent", FACTS["predecessor"]["agent"])
        heartbeat_proved = True
except Exception as error:
    failures.append(str(error))

compatibility_proved = False
rollout_proved = False
try:
    with case("current-server-deploys-real-nix-output-with-exact-release-agent-retained"):
        compatible = author("compatible")
        terminal(compatible)
        assert_store_absent(targetA, compatible["outputPath"])
        deploy(compatible, "compatible")
        receipt("upgrade.compatibilityExecutable", executable(targetA, "crystal-forge-agent", FACTS["predecessor"]["agent"]))
        expect_equal(targetA.succeed("cat /var/lib/upgrade-identity/agent.pub").strip(), agent_key, "old deployment preserves signing identity")
        compatibility_proved = heartbeat_proved
except Exception as error:
    failures.append(str(error))

try:
    with case("operator-deliberately-upgrades-agent-preserving-identity-and-history"):
        expect(compatibility_proved, "agent replacement is blocked: exact predecessor compatibility was not proved")
        # The old agent reports the new running state before the detached NixOS
        # switch finishes cleanup. Wait for its actual unit to release the
        # activation lock before starting a second operator-driven activation.
        targetA.wait_until_succeeds("test -z \"$(systemctl list-units --no-legend --plain "
            "--state=activating,active,deactivating 'crystal-forge-deploy-*.service')\"", timeout=180)
        targetA.succeed("systemctl stop crystal-forge-agent")
        upgrade(targetA)
        targetA.succeed("systemctl start crystal-forge-agent")
        receipt("upgrade.currentAgentExecutable", executable(targetA, "crystal-forge-agent", FACTS["current"]["agent"]))
        expect_equal(targetA.succeed("cat /var/lib/upgrade-identity/agent.pub").strip(), agent_key, "deliberate upgrade preserves private-key identity")
        expect_equal(sql_json(controller, "SELECT id::text, hostname, public_key, flake_id FROM systems WHERE id=" +
            literal(target["id"]) + "::uuid"), receipts()["upgrade.identity"], "deliberate upgrade preserves SystemID")
        rollout_proved = True
except Exception as error:
    failures.append(str(error))

try:
    with case("new-current-operation-completes-with-actual-target-outcome-and-report"):
        expect(rollout_proved, "current-agent operation is blocked: deliberate agent rollout was not proved")
        current = author("current")
        terminal(current)
        deploy(current, "current")
        # This source explicitly selects the current package. The preceding
        # compatibility candidate kept the released executable instead.
        executable(targetA, "crystal-forge-agent", FACTS["current"]["agent"])
        receipt("upgrade.currentOperationCompleted", True)
        run_browser(controller, "version-transition")
except Exception as error:
    failures.append(str(error))

try:
    with case("failed-or-incomplete-publication-cannot-authorize-deployment"):
        # D2 requires fail-closed authority and no protected side effect. It
        # does not prescribe HTTP 4xx. A status alone cannot prove this case.
        positive_phase = "current" if receipts().get("upgrade.currentOperationCompleted") else "compatible"
        positive = receipts().get("upgrade." + positive_phase)
        positive_request = receipts().get("upgrade." + positive_phase + ".request")
        positive_report = receipts().get("upgrade." + positive_phase + ".report")
        expect(bool(positive and positive_request and positive_report),
               "publication probe is blocked: no real successful current-server deployment positive control")
        control = sql_json(controller, "SELECT id::text, system_id::text, target_store_path, status "
            "FROM pending_system_deployments WHERE id=" + literal(positive_request["deployment_id"]) + "::uuid")
        expect_equal(control, [{"id": positive_request["deployment_id"], "system_id": target["id"],
            "target_store_path": positive["outputPath"], "status": "succeeded"}], "real positive-control deployment binding")
        who = api("GET", "/api/auth/whoami")
        expect_equal(who["user"]["id"], receipts()["upgrade.adminId"], "same authorized positive-control actor")
        expect_equal(who["roles"], ["Admin"], "publication probe actor authorization")
        cache_control = sql_json(controller, "SELECT id, cache_type, push_to, enabled, require_sigs FROM cache_destinations WHERE id=" +
            str(receipts()["upgrade.cacheId"]))
        expect_equal(cache_control, [{"id": receipts()["upgrade.cacheId"], "cache_type": "Nix",
            "push_to": "ssh-ng://cache@services", "enabled": True, "require_sigs": True}],
            "same configured mandatory signed-publication destination as positive control")
        receipt("upgrade.publicationPositiveControl", {"deployment": control[0], "commit": positive["commit"],
            "cache": cache_control[0], "agentReport": positive_report})
        # A real publication transport failure supplies no completion authority.
        # Restore the local cache even when a required assertion fails.
        services.succeed("systemctl stop sshd.service")
        try:
            unpublished = author("unpublished")
            changed = services.succeed("git -C /tmp/upgrade-source diff --name-only " +
                shlex.join([positive["commit"], unpublished["commit"]])).splitlines()
            # The current-agent rollout may add its package selector relative
            # to an earlier compatibility control; no other source/config input
            # may differ from the successful current-server workflow.
            expect(set(changed) <= {"nonce", "current-agent"} and "nonce" in changed,
                   "negative candidate differs from positive control beyond authored operation identity/agent rollout")
            def evaluated_negative_candidate():
                rows = sql_json(controller, "SELECT c.git_commit_hash, d.derivation_name, d.derivation_type, "
                    "d.expected_store_path, d.cf_agent_enabled, d.policy_requirements_met, a.status AS evaluation_status, "
                    "i.execution_mode FROM derivations d JOIN commits c ON c.id=d.commit_id "
                    "JOIN evaluation_attempts a ON a.commit_id=c.id AND a.attempt_number=c.evaluation_attempt_count "
                    "JOIN evaluation_attempt_inventory i ON i.evaluation_attempt_id=a.id WHERE d.id=" + str(unpublished["derivation_id"]))
                return rows if rows and rows[0]["evaluation_status"] == "complete" else None
            eligible = wait_for(evaluated_negative_candidate, "exact native evaluation finalized before publication probe", timeout=300)
            expect_equal(len(eligible), 1, "one exact evaluated negative candidate")
            expect_equal({k: eligible[0][k] for k in ("git_commit_hash", "derivation_name", "derivation_type",
                "expected_store_path", "cf_agent_enabled", "policy_requirements_met", "evaluation_status", "execution_mode")}, {
                "git_commit_hash": unpublished["commit"], "derivation_name": "targetA", "derivation_type": "nixos",
                "expected_store_path": unpublished["outputPath"], "cf_agent_enabled": True,
                "policy_requirements_met": True, "evaluation_status": "complete", "execution_mode": "native"},
                "valid source, target, policy and native-evaluation preconditions before publication fault")
            receipt("upgrade.incompletePublicationCandidate", eligible[0])
            def publication_failed():
                rows = sql_json(controller, "SELECT id::text, status, builder_id::text, dispatched_cache_destination_id, logs "
                    "FROM build_jobs WHERE derivation_id=" +
                    str(unpublished["derivation_id"]) + " ORDER BY created_at")
                pushes = sql_json(controller, "SELECT status FROM cache_push_jobs WHERE derivation_id=" +
                    str(unpublished["derivation_id"]))
                return {"jobs": rows, "publication": pushes} if rows and any(
                    r["status"] in ("failed", "success") for r in rows) else None
            outcome = wait_for(publication_failed, "actual failed publication transport outcome", timeout=600)
            receipt("upgrade.incompletePublication", outcome)
            executed = [row for row in outcome["jobs"] if row["status"] in ("failed", "success") and row["builder_id"]]
            expect(bool(executed), "publication probe has no actual executing builder")
            expect(all(row["dispatched_cache_destination_id"] == receipts()["upgrade.cacheId"] for row in executed),
                   "publication failure did not use the positive control's server-authorized destination")
            expect(not any(row["status"] == "success" for row in outcome["jobs"]),
                   "failed required publication became authoritative job completion")
            claimant = builderA if executed[-1]["builder_id"] == receipts()["upgrade.builderA.id"] else builderB
            expect(executed[-1]["builder_id"] in (receipts()["upgrade.builderA.id"], receipts()["upgrade.builderB.id"]),
                   "publication probe used an unregistered builder")
            claimant.succeed("nix-store --check-validity " + shlex.quote(unpublished["outputPath"]))
            expect_equal(claimant.succeed("cat " + shlex.quote(unpublished["outputPath"] + "/cf-it-source-nonce")).strip(),
                         unpublished["nonce"], "actual realization precedes failed publication")
            expect(any("Cache push failed after successful build" in (row["logs"] or "") for row in executed),
                   "negative operation failed before the actual publication boundary")
            expect(not any(p["status"] == "completed" for p in outcome["publication"]),
                   "failed transport became completed publication")
            assert_store_absent(services, unpublished["outputPath"])
            import uuid
            def protected_state():
                return {
                    "system": sql_json(controller, "SELECT id::text, public_key, flake_id, environment_id::text, "
                        "system_configuration_name, deployment_policy, desired_target FROM systems WHERE id=" + literal(target["id"]) + "::uuid"),
                    "deployments": sql_json(controller, "SELECT id::text, status, target_store_path, delivered_at, "
                        "requested_commit_id, requested_derivation_id FROM pending_system_deployments WHERE system_id=" +
                        literal(target["id"]) + "::uuid ORDER BY id"),
                    "candidateEvents": sql_json(controller, "SELECT e.id::text, e.event_type, e.deployment_id::text, "
                        "e.new_store_path, e.metadata FROM system_events e LEFT JOIN pending_system_deployments p "
                        "ON p.id=e.deployment_id WHERE e.system_id=" + literal(target["id"]) +
                        "::uuid AND (e.new_store_path=" + literal(unpublished["outputPath"]) + " OR p.requested_commit_id=" +
                        str(unpublished["commit_id"]) + ") ORDER BY e.id"),
                    "target": {
                        "running": targetA.succeed("readlink -f /run/current-system").strip(),
                        "profile": targetA.succeed("readlink -f /nix/var/nix/profiles/system").strip(),
                        "marker": targetA.succeed("cat /etc/cf-it-revision").strip(),
                    },
                }
            targetA.wait_until_succeeds("test -z \"$(systemctl list-units --no-legend --plain "
                "--state=activating,active,deactivating 'crystal-forge-deploy-*.service')\"", timeout=180)
            before = protected_state()
            expect_equal(before["candidateEvents"], [], "unpublished candidate has no prior authority events")
            assert_store_absent(targetA, unpublished["outputPath"])
            negative_request_id = str(uuid.uuid4())
            status, response = http(controller, "POST", "/api/v1/systems/" + target["id"] + "/deploy", {
                "commit_sha": unpublished["commit"], "action": "deploy", "request_id": negative_request_id,
            }, session="upgrade-admin", csrf=True)
            after = protected_state()
            reservation = sql_json(controller, "SELECT system_id::text, requested_commit_id, request_action, state, "
                "deployment_id::text FROM deployment_request_reservations WHERE request_id=" + literal(negative_request_id) + "::uuid")
            receipt("upgrade.incompletePublicationResponse", {"status": status, "body": response,
                "requestId": negative_request_id, "reservation": reservation,
                "protectedBefore": before, "protectedAfter": after})
            expect_equal(len(reservation), 1, "request passed authenticated shape/system/commit resolution before publication admission")
            expect_equal({key: reservation[0][key] for key in ("system_id", "requested_commit_id", "request_action", "deployment_id")},
                {"system_id": target["id"], "requested_commit_id": unpublished["commit_id"],
                 "request_action": "deploy", "deployment_id": None}, "valid intent reservation grants no deployment authority")
            expect(reservation[0]["state"] in ("reserved", "deploy_failed"), "unpublished request was queued or converted")
            expect_equal(after, before, "incomplete publication creates no deployment authority")
            expect(400 <= status < 600, "publication-incomplete request was accepted or redirected: " + str(status))
            # Exercise a subsequent genuine signed agent poll. An API error
            # must not conceal a delivered command or delayed target mutation.
            journal_cursor = targetA.succeed("journalctl -u crystal-forge-agent -n 1 --show-cursor --no-pager -o cat | "
                "awk '/^-- cursor:/ {print $3}'").strip()
            expect(bool(journal_cursor), "agent delivery observation has no journal cursor")
            targetA.succeed("systemctl restart crystal-forge-agent")
            def observed_poll():
                state = protected_state()
                if state != before:
                    return {"state": state, "agentResult": "protected state changed"}
                log = targetA.succeed("journalctl -u crystal-forge-agent --no-pager -o cat --after-cursor " +
                    shlex.quote(journal_cursor) + " --grep " + shlex.quote(
                        "No deployment needed|Already on target|Deployment started|Deployment failed|Heartbeat failed|server responded") + " || true").strip()
                return {"state": state, "agentResult": log} if log else None
            polled = wait_for(observed_poll, "real agent delivery result after rejected unpublished request", timeout=180)
            receipt("upgrade.incompletePublicationAgentPoll", polled)
            expect_equal(polled["state"], before, "no delayed authority grant, completion event or activation after genuine agent poll")
            expect("No deployment needed" in polled["agentResult"] or "Already on target" in polled["agentResult"],
                   "agent did not prove a successful fail-closed delivery poll")
            final_reservation = sql_json(controller, "SELECT state, deployment_id::text FROM deployment_request_reservations "
                "WHERE request_id=" + literal(negative_request_id) + "::uuid")
            expect_equal(len(final_reservation), 1, "negative intent identity remains attributable")
            expect(final_reservation[0]["deployment_id"] is None and final_reservation[0]["state"] in ("reserved", "deploy_failed"),
                   "unpublished intent acquired authority after genuine agent delivery poll")
            final_jobs = sql_json(controller, "SELECT id::text, status FROM build_jobs WHERE derivation_id=" + str(unpublished["derivation_id"]))
            final_publications = sql_json(controller, "SELECT status FROM cache_push_jobs WHERE derivation_id=" + str(unpublished["derivation_id"]))
            expect(not any(row["status"] == "success" for row in final_jobs), "unpublished job gained completion authority after request")
            expect(not any(row["status"] == "completed" for row in final_publications), "unverified publication became completed after request")
            receipt("upgrade.incompletePublicationDenied", status)
        finally:
            services.succeed("systemctl start sshd.service")
except Exception as error:
    failures.append(str(error))

expect(not failures, "mandatory Upgrade cases failed: " + " | ".join(failures))
