# Independent reporting/read authority: known preexisting artifact evidence is
# fixture setup, never evidence of evaluation, build, publication or activation.
# The full delivery-lifecycle remains a required sibling gate.
{harness, owner, name}: let
  inherit (harness) pkgs;
  artifact = pkgs.writeTextDir "failure-reporting-fixture" "Known preexisting reporting target; not a bootable deployment.\n";
in harness.mkScenario {
  inherit owner name;
  contracts = ["delivery.activation.recovery"];
  nodes.controller = {...}: {
    imports = [(harness.roles.controller {browser = true;})];
    environment.systemPackages = [pkgs.nodejs];
    environment.etc."cf-it/failure-report.cjs".source = ../fixtures/failure-report.cjs;
    environment.etc."cf-it/activation-callback.cjs".source = ../fixtures/activation-callback.cjs;
    environment.etc."cf-it/system_events.rs".source = ../../../packages/default/crates/cf-server/src/queries/system_events.rs;
    virtualisation.additionalPaths = [artifact harness.packages.agent];
    # Start only in the final callback fragment, after browser membership checks.
    # This representative heartbeat service runs the production agent normally;
    # stopping it must not prevent a separate transient unit's reporter running.
    systemd.services.cf-it-callback-heartbeat = {
      description = "Delivery callback independence heartbeat agent";
      wantedBy = [];
      path = [pkgs.nix pkgs.coreutils pkgs.util-linux pkgs.iproute2 pkgs.procps];
      environment.CRYSTAL_FORGE_CONFIG = "/run/cf-it-callback/config.toml";
      serviceConfig = {
        Type = "exec";
        ExecStart = "${harness.packages.agent}/bin/agent";
        Restart = "no";
      };
    };
    services.crystal-forge.flakes = {
      flake_polling_interval = "1h";
      commit_evaluation_interval = "1h";
      build_processing_interval = "1h";
    };
  };
  facts = {
    admin = harness.facts.localUserFromDesign {
      id = "u1"; username = "mreyes"; password = "cf-it-delivery-admin-1";
    };
    flake = {name = "failure-reporting-fixture";};
    target = "failure-reporting-fixture";
    outputPath = "${artifact}";
    agentBinary = "${harness.packages.agent}/bin/agent";
    sourceRevision = "5290000000000000000000000000000000000000";
  };
  script = pkgs.writeText "failure-reporting.py" (
    builtins.readFile ./failure-reporting.py + "\n"
    + builtins.readFile ./fragments/progress-query-performance.py + "\n"
    + builtins.readFile ./fragments/failure-detail.py + "\n"
    + builtins.readFile ./fragments/activation-callback.py
  );
  browserSpecs.failure-detail = pkgs.writeText "failure-reporting.cjs" ''
    exports.run = ctx => require("${./fragments/failure-detail.cjs}").run(ctx, require("${./fragments/activation.cjs}"));
  '';
  timeout = 900;
}
