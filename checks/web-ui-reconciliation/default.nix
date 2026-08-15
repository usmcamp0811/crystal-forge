{ lib, pkgs, inputs, ... }:
pkgs.testers.runNixOSTest {
  name = "crystal-forge-web-ui-reconciliation";

  nodes.machine = {
    services.nginx = {
      enable = true;
      virtualHosts."localhost" = {
        listen = [{ addr = "0.0.0.0"; port = 8080; }];
        root = "${pkgs.crystal-forge.web-ui}/public";
        locations."/" = {
          tryFiles = "$uri $uri/ /index.html";
        };
      };
    };

    networking.firewall.allowedTCPPorts = [ 8080 ];
    environment.systemPackages = [
      pkgs.curl
      pkgs.jq
      pkgs.nodejs
      pkgs.playwright-test
      pkgs.chromium
    ];
    environment.variables = {
      NODE_PATH = "${pkgs.playwright-test}/lib/node_modules";
      PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
    };
  };

  testScript = ''
    machine.wait_for_unit("nginx.service")
    machine.wait_for_open_port(8080)

    machine.succeed("curl -fsS http://127.0.0.1:8080/ > /tmp/index.html")
    machine.succeed("grep -E 'src=\"[^\"]+\\.js\"' /tmp/index.html")
    machine.succeed("test -n \"$(grep -oE 'src=\"[^\"]+\\.js\"' /tmp/index.html | cut -d'\"' -f2)\"")
    machine.succeed("test -n \"$(find ${pkgs.crystal-forge.web-ui}/public -type f -name '*.js' -print -quit)\"")
    machine.succeed("wasm=$(find ${pkgs.crystal-forge.web-ui}/public -type f -name '*.wasm' -print -quit); test -n \"$wasm\"; test \"$(od -An -tx1 -N4 \"$wasm\" | tr -d ' \\n')\" = 0061736d")
    machine.succeed("echo WEB_UI_DIST=${pkgs.crystal-forge.web-ui}/public; readlink -f ${pkgs.crystal-forge.web-ui}")

    machine.succeed("mkdir -p /tmp/web-ui-tests /tmp/screenshots")
    machine.succeed("cp ${../web-ui/tests/integration-test.js} /tmp/web-ui-tests/integration-test.js")
    machine.succeed("cp ${../web-ui/coverage-manifest.json} /tmp/web-ui-tests/coverage-manifest.json")
    machine.succeed("rm -f /tmp/web-ui-tests/integration.exit /tmp/screenshots/results.json /tmp/screenshots/fatal.json")
    machine.succeed(
        "nohup sh -c 'env CF_UI_TEST_PROFILE=ci_fast "
        "CF_UI_TEST_STEPS=20ac-stig-import-reconciliation-fixture "
        "CF_UI_TEST_STANDALONE=1 "
        "node /tmp/web-ui-tests/integration-test.js http://127.0.0.1:8080 /tmp/screenshots; "
        "status=$?; printf \"%s\\n\" \"$status\" > /tmp/web-ui-tests/integration.exit' "
        "> /tmp/web-ui-tests/integration.log 2>&1 </dev/null &"
    )
    machine.wait_until_succeeds(
        "test -f /tmp/screenshots/results.json -o -f /tmp/screenshots/fatal.json -o -f /tmp/web-ui-tests/integration.exit",
        timeout=180,
    )
    print(machine.succeed("cat /tmp/web-ui-tests/integration.log"))

    if machine.execute("test -f /tmp/screenshots/fatal.json")[0] == 0:
        raise Exception(machine.succeed("cat /tmp/screenshots/fatal.json"))
    if machine.execute("test -f /tmp/screenshots/results.json")[0] != 0:
      exit_code = machine.succeed("cat /tmp/web-ui-tests/integration.exit").strip()
      raise Exception(f"integration process exited before results.json (exit code {exit_code})")

    for diagnostic in [
        "20ac-reconciliation-dom.html",
        "20ac-shared-group-missing.png",
    ]:
        if machine.execute(f"test -f /tmp/screenshots/{diagnostic}")[0] == 0:
            machine.copy_from_vm(f"/tmp/screenshots/{diagnostic}", "screenshots")

    machine.succeed("jq -e 'length == 1 and .[0].name == \"20ac-stig-import-reconciliation-fixture\" and .[0].ok == true' /tmp/screenshots/results.json")
    for screenshot in [
        "20ac-stig-import-reconciliation-fixture--dark.png",
        "20ac-stig-import-reconciliation-fixture--light.png",
    ]:
        machine.succeed(f"test -s /tmp/screenshots/{screenshot}")
        machine.copy_from_vm(f"/tmp/screenshots/{screenshot}", "screenshots")
    machine.copy_from_vm("/tmp/screenshots/results.json", "screenshots")
  '';
}
