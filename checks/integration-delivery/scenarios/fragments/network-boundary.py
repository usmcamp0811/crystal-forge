# The endpoint is test-owned and positively reachable through the same route
# from a narrow control identity. Product actors remain blocked; exact deny
# counters distinguish boundary rejection from an unavailable endpoint.
import re


def probe_counter(node, family):
    text = node.succeed(("ip6tables-save" if family == 6 else "iptables-save") + " -c")
    lines = [line for line in text.splitlines() if "cf-it-probe-v" + str(family) in line]
    expect_equal(len(lines), 1, "one exact probe rejection rule")
    return int(re.match(r"\[(\d+):", lines[0]).group(1))


with case("network-boundary-rejects-controlled-ipv4-and-ipv6-outside-paths"):
    services.wait_for_unit("network-probe.service")
    services.wait_for_open_port(9080)
    for node, role in ((controller, "controller"), (builderA, "builderA"),
                       (builderB, "builderB"), (targetA, "targetA")):
        for family, address in ((4, "192.168.2.4"), (6, "2001:db8:2::4")):
            route = json.loads(node.succeed("ip -j -" + str(family) + " route get " + address))[0]
            expect_equal(route["dev"], "eth2", role + " probe uses controlled outside interface")
            url = "http://" + ("[" + address + "]" if family == 6 else address) + ":9080/"
            command = "curl --noproxy '*' --http1.0 -fsS --connect-timeout 3 --max-time 5 " + shlex.quote(url)
            control = node.succeed("runuser -u net-control -- " + command).strip()
            expect_equal(control, "CF-OWNED-OUTSIDE-PROBE", "live same-route positive control")
            before = probe_counter(node, family)
            status, _ = node.execute(command)
            expect(status != 0, role + " reached outside probe as a product actor")
            expect(probe_counter(node, family) > before, "probe failed without hitting intended boundary")
            receipt("delivery.network." + role + ".v" + str(family),
                    {"interface": route["dev"], "controlReachable": True, "boundaryRejected": True})
    # These are real local-role connections, not an exception to outside rules.
    expect_equal(delivery_api("GET", "/api/auth/whoami")["user"]["id"],
                 receipts()["delivery.adminId"], "local API remains usable")
    for node in (builderA, builderB, targetA):
        status, body = http(node, "GET", "/api/auth/setup-status", base="http://controller:3000")
        expect_equal(status, 200, "role-to-controller API remains reachable")
        expect_equal(body["auth_mode"], "local", "real local controller response")
    advertised = controller.succeed("git ls-remote " + shlex.quote(FACTS["flake"]["url"]) + " refs/heads/main")
    expect_equal(advertised.split()[0], receipts()["delivery.sourceRevision"], "local Git remains reachable")
    for node in (controller, builderA, builderB):
        node.succeed("runuser -u crystal-forge -- ssh -o ConnectTimeout=5 cache@services true")
    receipt("delivery.localRoleTrafficPreserved", True)
