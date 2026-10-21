# Separate whole-closure proof on the existing target. Ordinary activation may
# share immutable prerequisites, but this receiving database/store is empty and
# trusts only the configured CF publisher keys. Test-side receiver signing is
# neither trusted here nor used to repair destination publication metadata.
with case("populated-cache-serves-complete-cf-trusted-closure-to-cold-target-store"):
    nix = shlex.quote(FACTS["receiverNix"] + "/bin/nix")
    nix_store = shlex.quote(FACTS["receiverNix"] + "/bin/nix-store")
    output_path = receipts()["delivery.outputPath"]
    output = shlex.quote(output_path)
    receiver = "unix:///run/cf-it-cold-acquisition/socket"
    root = "/var/lib/cf-it-cold-acquisition"
    # Nix resolves ssh itself and rejects inaccessible PATH entries rather than
    # skipping root's profile. Give this unprivileged reader its own environment.
    reader = ("runuser -u cf-cold-reader -- env PATH=/run/current-system/sw/bin "
              "HOME=/var/lib/cf-cold-reader XDG_CACHE_HOME=/var/lib/cf-cold-reader/.cache "
              "NIX_USER_CACHE_DIR=/var/lib/cf-cold-reader/.cache/nix NIX_USER_CONF_FILES=/dev/null ")
    keys = receipts()["delivery.publisherPublicKeys"]
    expect(all(not key.startswith("delivery-receiver:") for key in keys), "receiver-test key contaminated target trust")
    _vm_write(targetA, "/run/cf-it-publisher-keys.pub", " ".join(keys) + "\n")
    targetA.succeed("install -d -m 0700 -o cf-cold-reader -g cf-cold-reader /var/lib/cf-cold-reader/.ssh && "
        + reader + "ssh-keygen -q -t ed25519 -N '' -f /var/lib/cf-cold-reader/.ssh/read-key")
    public_key = targetA.succeed("cat /var/lib/cf-cold-reader/.ssh/read-key.pub").strip()
    _vm_write(targetA, "/var/lib/cf-cold-reader/.ssh/known_hosts",
              "services " + receipts()["delivery.cacheHostPublicKey"] + "\n")
    _vm_write(targetA, "/var/lib/cf-cold-reader/.ssh/config", "Host services\n  User cache\n"
        "  IdentityFile /var/lib/cf-cold-reader/.ssh/read-key\n  IdentitiesOnly yes\n"
        "  BatchMode yes\n  StrictHostKeyChecking yes\n"
        "  UserKnownHostsFile /var/lib/cf-cold-reader/.ssh/known_hosts\n")
    targetA.succeed("chown -R cf-cold-reader:cf-cold-reader /var/lib/cf-cold-reader/.ssh && "
                    "chmod 600 /var/lib/cf-cold-reader/.ssh/config /var/lib/cf-cold-reader/.ssh/known_hosts")
    authorized = services.succeed("cat /var/lib/cache/.ssh/authorized_keys")
    _vm_write(services, "/var/lib/cache/.ssh/authorized_keys", authorized + public_key + "\n")
    services.succeed("chown cache:cache /var/lib/cache/.ssh/authorized_keys && chmod 600 /var/lib/cache/.ssh/authorized_keys")
    targetA.succeed(reader + "ssh -o ConnectTimeout=5 cache@services true")
    targetA.succeed("systemctl start cf-it-cold-acquisition.service")
    targetA.wait_until_succeeds("test -S /run/cf-it-cold-acquisition/socket")
    info = json.loads(targetA.succeed(reader + nix + " store info --json --store " + shlex.quote(receiver)))
    expect_equal(info["trusted"], False, "cold target reader cannot bypass signatures")
    closure = services.succeed("nix-store --query --requisites " + output).splitlines()
    expect_equal(targetA.succeed(reader + nix_store + " --store " + shlex.quote(receiver) +
                                " --query --all").splitlines(), [], "receiving store starts with no valid paths")
    # Check every physical path, not only validity: the main VM has an unrelated
    # prepared store, but the rooted receiving store must contain none of it.
    targetA.succeed("for path in " + shlex.join(closure) + "; do test ! -e " + root +
                    '"$path" && test ! -L ' + root + '"$path" || exit 1; done')
    dependencies = [p for p in closure if p != output_path]
    expect(bool(dependencies), "real system closure has no required dependencies")
    dependency = dependencies[0]
    for path in (output_path, dependency):
        targetA.fail(reader + nix_store + " --store " + shlex.quote(receiver) + " --check-validity " + shlex.quote(path))
        targetA.succeed("test ! -e " + shlex.quote(root + path) + " && test ! -L " + shlex.quote(root + path))
    targetA.succeed(reader + nix + " copy --from " + shlex.quote(FACTS["cacheUrl"]) +
                    " --to " + shlex.quote(receiver) + " " + output, timeout=300)
    targetA.succeed(reader + nix + " store verify --store " + shlex.quote(receiver) +
        " --recursive --no-contents --sigs-needed 1 --option trusted-public-keys " +
        shlex.quote(" ".join(keys)) + " --option extra-trusted-public-keys '' " + output, timeout=180)
    raw = json.loads(targetA.succeed(reader + nix + " path-info --store " + shlex.quote(receiver) +
                                    " --recursive --json " + output))
    received_paths = [p["path"] for p in raw] if isinstance(raw, list) else list(raw)
    expect_equal(set(received_paths), set(closure), "cold target acquired every required closure path")
    # Retain the exact source and received sets outside browser receipts. The
    # manifest establishes membership without copying CF's expected responses.
    with open(OUT / "closure-acquisition.json", "w") as manifest:
        json.dump({"source": FACTS["cacheUrl"], "receivingStore": receiver, "root": root,
                   "reader": "cf-cold-reader", "clientTrusted": info["trusted"],
                   "trustedPublicKeys": keys, "absentBefore": sorted(closure),
                   "sourceClosure": sorted(closure), "validAfter": sorted(received_paths)}, manifest, indent=2)
    expect_equal(targetA.succeed("cat " + shlex.quote(root + output_path + "/cf-it-source-nonce")).strip(),
                 receipts()["delivery.sourceNonce"], "cold receipt has exact CF-produced content")
    assert_store_absent(targetA, output_path)
    receipt("delivery.coldClosureAcquisition", {"outputPath": output_path, "source": FACTS["cacheUrl"],
        "clientTrusted": False, "missingDependencyBefore": dependency, "closurePathCount": len(closure),
        "publisherKeyNames": [key.split(":")[0] for key in keys], "ordinaryTargetOutputStillCold": True})
