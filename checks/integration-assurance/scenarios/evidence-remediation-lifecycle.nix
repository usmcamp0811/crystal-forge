# One fresh mutable lifecycle. Delivery contributes immutable role/setup code,
# never its VM, database, cookies, outputs, or receipts. Scanning assertions and
# deployment prerequisites remain separate responsibilities.
{harness, owner, name}: let
  inherit (harness) pkgs lib;
  native = import ../fixtures/source.nix {inherit harness;};
  data = import ../fixtures/vuln-data {inherit pkgs;};
  delivery = ../../integration-delivery;
  tls = pkgs.runCommand "assurance-nvd-tls" {nativeBuildInputs = [pkgs.openssl];} ''
    mkdir -p "$out"
    openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
      -subj '/CN=Assurance test CA' -addext 'basicConstraints=critical,CA:TRUE' \
      -addext 'keyUsage=critical,keyCertSign,cRLSign' -addext 'subjectKeyIdentifier=hash' \
      -keyout "$out/ca.key" -out "$out/ca.crt"
    openssl req -newkey rsa:2048 -nodes -subj '/CN=nvd.nist.gov' \
      -keyout "$out/nvd.key" -out request.csr
    printf '%s\n' 'subjectAltName=DNS:nvd.nist.gov' 'extendedKeyUsage=serverAuth' \
      'basicConstraints=critical,CA:FALSE' 'keyUsage=critical,digitalSignature,keyEncipherment' \
      'subjectKeyIdentifier=hash' 'authorityKeyIdentifier=keyid,issuer' > extensions
    openssl x509 -req -in request.csr -CA "$out/ca.crt" -CAkey "$out/ca.key" \
      -CAcreateserial -days 3650 -extfile extensions -out "$out/nvd.crt"
    openssl verify -x509_strict -purpose sslserver -CAfile "$out/ca.crt" "$out/nvd.crt"
    # Disposable test keys only; no production key or credential is imported.
  '';
  cveSpec = ./fragments/cve.cjs;
  wrapper = phase: pkgs.writeText "cve-${phase}.cjs" ''
    exports.run = require("${cveSpec}").${phase};
  '';
in harness.mkScenario {
  inherit owner name;
  contracts = ["assurance.scanning.cve" "assurance.remediation.triage" "assurance.remediation.poam-plan" "assurance.remediation.poam-verification"];
  nodes = {
    controller = {...}: {
      imports = [(harness.roles.controller {browser = true;}) native.prepared];
      environment.systemPackages = [pkgs.git];
      services.crystal-forge = {
        server = {eval_workers = 1; eval_max_memory_mb = 1024;};
        build.remote_execution_strategy = "server_derivation";
        flakes = {flake_polling_interval = "1h"; commit_evaluation_interval = "2s"; build_processing_interval = "2s";};
        vulnix.poll_interval = "2s";
      };
      # The packaged server wrapper adds vulnix to PATH. Make only that optional
      # executable unavailable in the server's mount namespace, preserving its
      # real coordination/maintenance and the builder's unchanged real scanner.
      systemd.services.crystal-forge-server.serviceConfig.InaccessiblePaths = ["${pkgs.vulnix}/bin/vulnix"];
    };
    services = {...}: {
      imports = [(harness.roles.services {}) native.prepared (data.servicesModule {
        sslCertificate = "${tls}/nvd.crt"; sslCertificateKey = "${tls}/nvd.key";
      })];
      environment.etc."cf-it/source".source = native.source;
      # Only public feed/test-CA material is consumed by the scanner clients.
    };
    builderA = {nodes, ...}: {
      imports = [(harness.roles.builder {name = "builderA";}) native.prepared];
      services.crystal-forge = {
        build.cve_scanning_enabled = lib.mkForce true;
        vulnix.poll_interval = "2s";
      };
      security.pki.certificateFiles = ["${tls}/ca.crt"];
      networking.extraHosts = "${nodes.services.networking.primaryIPAddress} nvd.nist.gov";
      systemd.services.crystal-forge-builder.environment.REQUESTS_CA_BUNDLE = "/etc/ssl/certs/ca-certificates.crt";
    };
    builderB = {...}: {imports = [(harness.roles.builder {name = "builderB";}) native.prepared];};
    targetA = {...}: {
      imports = [(harness.roles.target {}) (delivery + "/fixtures/network-isolation.nix")
        (delivery + "/fixtures/target-baseline.nix")];
      virtualisation.additionalPaths = [native.baseline.config.system.build.toplevel];
    };
  };
  facts = {
    admin = harness.facts.localUserFromDesign {id = "u1"; username = "mreyes"; password = "cf-it-assurance-admin-1";};
    flake = {name = "assurance-source"; url = "http://services/delivery.git"; branch = "main";};
    target = "targetA"; cacheUrl = "ssh-ng://cache@services";
    receiverNix = "${pkgs.crystal-forge.default.evaluatorNix}";
    cve = data.facts;
    feedCommands = data.feedCommands;
    cveViewer = harness.facts.localUserFromDesign {id = "u2"; username = "echan"; password = "cf-it-assurance-viewer-1";};
    cveOperator = harness.facts.localUserFromDesign {id = "u3"; username = "rpatel"; password = "cf-it-assurance-operator-1";};
    remediation = {
      title = "${data.facts.cveId} - patch ${data.facts.package} on targetA";
      initialPlan = "Deploy the independently scanned corrected demo package on targetA.";
      updatedPlan = "Activate demo 2.0, verify the exact running clean scan, then request authoritative closure.";
      note = "Maintenance approved for the isolated Assurance target.";
      milestone = "Activate and verify corrected generation";
      targetDate = "2027-02-01";
    };
  };
  script = pkgs.writeText "evidence-remediation-lifecycle.py" (
    ''
      start_all()
      # The authored 2026 advisory must be in the scanner's six-year feed span.
      # Set every disposable VM before creating authentication/evidence records.
      for node in machines:
          node.succeed("date -s '2027-01-01 12:00:00 UTC'")
    '' + builtins.readFile (delivery + "/scenarios/delivery-lifecycle.py") + "\n"
    + builtins.readFile ./fragments/cve.py + "\n"
    + builtins.readFile ./fragments/triage.py + "\n"
    + builtins.readFile ./fragments/poam-plan.py + "\n"
    + builtins.readFile ./fragments/verification.py + "\n"
    + builtins.readFile ./evidence-remediation-lifecycle.py
  );
  browserSpecs = {
    cve = cveSpec;
    cve-scan = wrapper "scan"; cve-current = wrapper "current";
    cve-missing = wrapper "missing"; cve-historical = wrapper "historical";
    cve-clean = wrapper "clean"; cve-failed = wrapper "failed";
    cve-retry = wrapper "retry"; cve-read-error = wrapper "readError";
    cve-viewer = wrapper "viewer";
    triage-create = ./fragments/triage.cjs;
    poam-maintain = ./fragments/poam-plan.cjs;
    poam-verify-close = ./fragments/verification.cjs;
    deploy-first = pkgs.writeText "assurance-deploy-first.cjs" ''
      exports.run = ctx => require("${delivery + "/scenarios/fragments/activation.cjs"}").deploy(ctx, "first");
    '';
    deploy-second = pkgs.writeText "assurance-deploy-second.cjs" ''
      exports.run = ctx => require("${delivery + "/scenarios/fragments/activation.cjs"}").deploy(ctx, "second");
    '';
  };
  timeout = 1800;
}
