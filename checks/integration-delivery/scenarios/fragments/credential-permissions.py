# Requires the native build/publication receipts, services' runtime host key and
# authorized publisher keys. Reuses that actual SSH store and published output.
# Keys never leave their originating guest; receipts contain metadata/digests.
def credential_metadata(node, paths):
    result = {}
    for path in paths:
        quoted = shlex.quote(path)
        result[path] = {
            "stat": node.succeed("stat -Lc '%u:%g:%a' " + quoted).strip(),
            "lstat": node.succeed("stat -c '%u:%g:%a:%F' " + quoted).strip(),
            "sha256": node.succeed("sha256sum " + quoted).split()[0],
        }
    return result


def credential_cache_probe(node):
    # Query the remote store, even though the build output is locally valid.
    # A local path-info would never exercise SSH authentication.
    command = ("runuser -u crystal-forge -- env HOME=/var/lib/crystal-forge "
        "PATH=/run/current-system/sw/bin NIX_SSHOPTS=" + shlex.quote(
        "-o IdentitiesOnly=yes -o IdentityAgent=none -o BatchMode=yes "
        "-o PasswordAuthentication=no -o KbdInteractiveAuthentication=no "
        "-o StrictHostKeyChecking=yes -o ConnectTimeout=5 "
        "-o UserKnownHostsFile=/var/lib/crystal-forge/cache-known-hosts "
        "-i /var/lib/crystal-forge/cache-ssh") + " " +
        shlex.quote(FACTS["receiverNix"] + "/bin/nix") +
        " path-info --store " + shlex.quote(FACTS["cacheUrl"]) +
        " --json " + shlex.quote(receipts()["delivery.outputPath"]) + " 2>&1")
    status, diagnostic = node.execute(command)
    return {"exit": status, "diagnostic": diagnostic}


with case("private-cache-credentials-survive-real-activation-and-service-restarts"):
    output = receipts()["delivery.outputPath"]
    for node, name, unit in ((controller, "controller", "crystal-forge-server"),
                             (builderA, "builderA", "crystal-forge-builder"),
                             (builderB, "builderB", "crystal-forge-builder")):
        prefix = "delivery.credentialPermissions." + name
        node.succeed("test \"$(systemctl show -p User --value " + unit + ")\" = crystal-forge")
        identity = node.succeed("runuser -u crystal-forge -- id -u; "
            "runuser -u crystal-forge -- id -g; "
            "stat -c '%u:%g' /proc/$(systemctl show -p MainPID --value " + unit + ")").splitlines()
        expect_equal(identity[2], identity[0] + ":" + identity[1], "actual service process identity")
        # These controls catch startup's separate recursive .cache/.config rules.
        # Model an existing secret-manager target owned by root, readable by CF.
        node.succeed("install -d -m 0755 /run/cf-it-secrets && "
            "(umask 077; ssh-keygen -q -t ed25519 -N '' -f /run/cf-it-secrets/external) && "
            "chgrp crystal-forge /run/cf-it-secrets/external && "
            "chmod 0440 /run/cf-it-secrets/external && "
            "ln -s /run/cf-it-secrets/external /var/lib/crystal-forge/.config/external-key && "
            "runuser -u crystal-forge -- sh -ec 'umask 077; "
            "ssh-keygen -q -t ed25519 -N \"\" -f /var/lib/crystal-forge/.cache/private-key; "
            "ssh-keygen -q -t ed25519 -N \"\" -f /var/lib/crystal-forge/.config/private-key'")
        paths = ["/var/lib/crystal-forge/cache-ssh", "/var/lib/crystal-forge/cache-sign.key",
                 "/var/lib/crystal-forge/.cache/private-key",
                 "/var/lib/crystal-forge/.config/private-key",
                 "/run/cf-it-secrets/external", "/var/lib/crystal-forge/.config/external-key"]
        if name == "builderA":
            paths += ["/var/lib/crystal-forge/builder-api.key"]
        if name == "builderB":
            paths += ["/var/lib/crystal-forge/builder-api.key", "/run/cf-it-secrets/cache-ssh"]
            node.succeed("grep -F 'ssh_key_path = \"/var/lib/crystal-forge/cache-ssh\"' /var/lib/crystal-forge/config.toml")
        else:
            paths += ["/var/lib/crystal-forge/.ssh/id_ed25519"]
        before = credential_metadata(node, paths)
        expect_equal(before[paths[0]]["stat"], identity[0] + ":" + identity[1] + ":600",
                     "legitimate inside-state-tree private cache key")
        node.fail("runuser -u nobody -- test -r /var/lib/crystal-forge/cache-ssh")
        probe = credential_cache_probe(node)
        receipt(prefix + ".before", {"identity": identity, "files": before, "probe": probe,
            "generatedTmpfiles": node.succeed("grep -F crystal-forge /etc/tmpfiles.d/00-nixos.conf")})
        expect_equal(probe["exit"], 0, "authenticated cache usable before activation: " + probe["diagnostic"])
        expect(output in probe["diagnostic"], "remote cache returned another output")
        for cycle in range(2):
            # Use the real switch path and NixOS's generated reactivation unit.
            # Reapply that unit explicitly on same-generation repeats: unchanged
            # restartTriggers can otherwise leave it active without rerunning.
            # Calling /activate alone does not execute tmpfiles on this NixOS.
            activation_status, _ = node.execute(
                "/run/current-system/bin/switch-to-configuration test "
                "> /run/cf-it-activation.log 2>&1")
            tmpfiles_status, tmpfiles_diagnostic = node.execute(
                "systemctl restart systemd-tmpfiles-resetup.service 2>&1")
            after_activation = credential_metadata(node, paths)
            after_probe = credential_cache_probe(node)
            nobody_status, _ = node.execute("runuser -u nobody -- test -r /var/lib/crystal-forge/cache-ssh")
            observed = {"afterActivation": after_activation, "activationProbe": after_probe,
                        "nobodyReadExit": nobody_status, "activationExit": activation_status,
                        "tmpfilesExit": tmpfiles_status, "tmpfilesDiagnostic": tmpfiles_diagnostic}
            receipt(prefix + ".activation" + str(cycle), observed)
            # Probe first so the failing baseline preserves OpenSSH's actual
            # diagnostic/exit as well as the changed mode and unchanged digest.
            expect_equal(after_probe["exit"], 0, "authenticated cache after real activation: " + after_probe["diagnostic"])
            expect_equal(activation_status, 0, "real NixOS activation")
            expect_equal(tmpfiles_status, 0, "generated tmpfiles reactivation unit")
            expect_equal(after_activation, before, "activation preserves credential owner/group/mode/content")
            expect(nobody_status != 0, "unrelated nobody can read private cache key")
            node.succeed("systemctl restart " + unit)
            node.wait_for_unit(unit + ".service")
            after_restart = credential_metadata(node, paths)
            restarted_probe = credential_cache_probe(node)
            receipt(prefix + ".restart" + str(cycle), {"files": after_restart, "probe": restarted_probe})
            expect_equal(after_restart, before, "restart preserves credential owner/group/mode/content")
            expect_equal(restarted_probe["exit"], 0, "authenticated cache after service restart")
            node.succeed("test -L /var/lib/crystal-forge/.config/external-key && "
                         "test \"$(readlink /var/lib/crystal-forge/.config/external-key)\" = /run/cf-it-secrets/external")
            node.fail("runuser -u nobody -- test -r /var/lib/crystal-forge/cache-ssh")
            for directory in ("/var/lib/crystal-forge/workdir", "/var/lib/crystal-forge/tmp",
                              "/var/lib/crystal-forge/builds", "/var/lib/crystal-forge/.cache/nix",
                              "/var/lib/crystal-forge/.config/attic", "/var/cache/crystal-forge-nix"):
                node.succeed("runuser -u crystal-forge -- sh -ec " + shlex.quote(
                    "mkdir " + directory + "/permission-write-" + str(cycle) +
                    "; printf fresh > " + directory + "/permission-write-" + str(cycle) + "/state"))
            node.succeed("runuser -u crystal-forge -- logger -t cf-it-permissions fresh-state-write-" + str(cycle))
            node.succeed("journalctl -t cf-it-permissions --no-pager | grep -F fresh-state-write-" + str(cycle))
