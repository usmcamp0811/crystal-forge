# Native publication has completed. Reuse its exact output, but not its warm
# destination or trusted SSH identity, to prove signature enforcement. All
# destination writes cross an untrusted Unix-daemon connection in services.
def receiver_metadata(store, path):
    raw = json.loads(services.succeed(
        "runuser -u cache -- " + shlex.quote(FACTS["receiverNix"] + "/bin/nix") +
        " path-info --store " + shlex.quote(store) + " --json " + shlex.quote(path)))
    return raw[0] if isinstance(raw, list) else raw[path]


with case("publication-rejects-untrusted-signature-then-accepts-same-signed-output"):
    output_path = receipts()["delivery.outputPath"]
    output = shlex.quote(output_path)
    nix = shlex.quote(FACTS["receiverNix"] + "/bin/nix")
    nix_store = shlex.quote(FACTS["receiverNix"] + "/bin/nix-store")
    receiver = "unix:///run/cf-it-signature-receiver/socket"
    root = "/var/lib/cf-it-signature-receiver"
    key = "/run/cf-it-receiver-sign.key"
    services.succeed("umask 077; " + nix_store + " --generate-binary-cache-key delivery-receiver " +
                     key + " /run/cf-it-receiver-sign.pub && "
                     "systemctl start cf-it-signature-receiver.service")
    services.wait_for_unit("cf-it-signature-receiver.service")
    services.wait_until_succeeds("test -S /run/cf-it-signature-receiver/socket")
    info = json.loads(services.succeed("runuser -u cache -- " + nix +
                      " store info --json --store " + shlex.quote(receiver)))
    expect_equal(info["trusted"], False, "receiver client cannot bypass signatures")
    before = receiver_metadata("daemon", output_path)
    expect(not before.get("ca"), "signature probe must be input-addressed, not content-addressed")
    expect(bool(before.get("signatures")), "normal publication must supply a source signature")
    expect(any(s.startswith("delivery-builderA:") for s in before["signatures"]),
           "CF's assigned builder did not sign its published output")
    # Verify CF's signature before the receiver test adds any source signatures.
    # Test-side signing below proves receiver enforcement, not CF publication.
    services.succeed(nix + " store verify --store daemon --no-contents --sigs-needed 1 "
                     "--option trusted-public-keys " + shlex.quote(receipts()["delivery.publisherPublicKeys"][1]) +
                     " " + output)
    receipt("delivery.cfPublication", {"outputPath": output_path, "narHash": before["narHash"],
        "signatures": before["signatures"], "builderId": receipts()["delivery.builderA.id"],
        "jobId": receipts()["delivery.jobId"], "cacheId": receipts()["delivery.cacheId"]})
    expect(not any(s.startswith("delivery-receiver:") for s in before["signatures"]),
           "receiver already trusts target before negative test")
    closure = services.succeed(nix_store + " --store daemon --query --requisites " + output).splitlines()
    expect(output_path in closure, "source closure omitted its own output")
    dependencies = [p for p in closure if p != output_path]
    expect(bool(dependencies), "real system output must have dependencies")
    for offset in range(0, len(dependencies), 100):
        paths = " ".join(shlex.quote(p) for p in dependencies[offset:offset + 100])
        # Do not sign recursively: only dependencies may gain receiver trust
        # before the target's rejection. No privileged destination preseed.
        services.succeed(nix + " store sign --store daemon --key-file " + key + " " + paths)
        services.succeed("runuser -u cache -- " + nix + " copy --from daemon --to " +
                         shlex.quote(receiver) + " " + paths)
        services.succeed("runuser -u cache -- " + nix_store + " --store " +
                         shlex.quote(receiver) + " --check-validity " + paths)

    def require_receiver_absence():
        services.fail("runuser -u cache -- " + nix_store + " --store " +
                      shlex.quote(receiver) + " --check-validity " + output)
        services.succeed("test ! -e " + shlex.quote(root + output_path) +
                         " && test ! -L " + shlex.quote(root + output_path))

    require_receiver_absence()
    copy = "runuser -u cache -- " + nix + " copy --from daemon --to " + shlex.quote(receiver) + " " + output
    status, error = services.execute(copy + " 2>&1")
    expect(status != 0, "untrusted-signature target was accepted")
    expect("cannot add path '" + output_path + "' because it lacks a signature by a trusted key" in error,
           "copy failed for a reason other than target signature enforcement: " + error)
    require_receiver_absence()
    receipt("delivery.signatureRejected", {"outputPath": output_path, "clientTrusted": info["trusted"],
                                           "dependenciesValid": len(dependencies), "diagnostic": error})
    services.succeed(nix + " store sign --store daemon --key-file " + key + " " + output)
    signed = receiver_metadata("daemon", output_path)
    services.succeed(copy)
    received = receiver_metadata(receiver, output_path)
    for field in ("narHash", "narSize", "references", "ca"):
        expect_equal(signed.get(field), before.get(field), "signing preserves source " + field)
        expect_equal(received.get(field), before.get(field), "receiver preserves " + field)
    expect(any(s.startswith("delivery-receiver:") for s in received["signatures"]),
           "received output lacks the newly trusted signature")
    expect_equal(services.succeed("cat " + shlex.quote(root + output_path + "/cf-it-source-nonce")).strip(),
                 receipts()["delivery.sourceNonce"], "received contents match authored source")
    receipt("delivery.signatureAccepted", {"outputPath": output_path, "narHash": received["narHash"],
                                           "signatures": received["signatures"]})
