{inputs, lib, pkgs, ...}: let
  revision = "12cbeca141f46e1ade76728bce8adc447f2166c6";
  python = pkgs.python3.withPackages (p: [p.psycopg p.requests]);
  atticNode = lib.crystal-forge.makeAtticCacheNode {inherit pkgs lib;};
  driver = pkgs.crystal-forge.default.cf-builder-drv.overrideAttrs (_: {
    pname = "cf-attic-publication-driver";
    cargoBuildFlags = ["--package" "cf-builder" "--example" "attic_publication" "--example" "attic_bulk_publication"];
    doCheck = false;
    installPhase = ''
      mkdir -p "$out/bin"
      cp target/*/release/examples/attic_publication "$out/bin/attic-publication"
      cp target/*/release/examples/attic_bulk_publication "$out/bin/attic-bulk-publication"
    '';
    postFixup = "";
  });
  serverDriver = pkgs.crystal-forge.default.cf-server-core-drv.overrideAttrs (_: {
    pname = "cf-attic-server-publication-driver";
    # Crane interpolates its Cargo command before overrideAttrs. Override the
    # actual phase, not an environment attribute that the phase never reads.
    buildPhase = ''
      runHook preBuild
      cargoWithProfile build --locked --package cf-server --example attic_publication
    '';
    doCheck = false;
    installPhase = ''
      mkdir -p "$out/bin"
      cp target/release/examples/attic_publication "$out/bin/attic-server-publication"
    '';
    postFixup = "";
  });
in assert pkgs.attic-server.src.rev == revision;
pkgs.testers.runNixOSTest {
  name = "crystal-forge-attic-publication";
  skipLint = true;
  skipTypeCheck = true;
  globalTimeout = 600;
  nodes.machine = {lib, ...}: {
    imports = [atticNode];
    system.stateVersion = "26.05";
    virtualisation = {memorySize = lib.mkForce 3072; cores = 2; useNixStoreImage = true;};
    nix.settings = {experimental-features = ["nix-command" "flakes"]; substituters = lib.mkForce [];};
    environment.systemPackages = [python driver serverDriver pkgs.attic-client pkgs.nix];
    environment.etc."attic-publication.py".source = ./test.py;
    # Replace token-in-argv fixture startup with runtime protected config.
    systemd.services.attic-setup.wantedBy = lib.mkForce [];
    systemd.services.attic-debug.wantedBy = lib.mkForce [];
    services.postgresql.authentication = lib.mkForce "local all all trust\nhost all all 127.0.0.1/32 trust\nhost all all ::1/128 trust";
    systemd.services.atticd.environment.RUST_LOG = lib.mkForce "error";
    services.nginx = {
      enable = true;
      commonHttpConfig = ''
        map $http_authorization $publication_auth { "" false; default true; }
        log_format publication_safe escape=json '{"authorization":$publication_auth,"method":"$request_method","status":$status}';
      '';
      virtualHosts.publication = {
        listen = [{addr = "127.0.0.1"; port = 8081;}];
        extraConfig = ''access_log /var/log/nginx/publication.json publication_safe; error_log /dev/null;'';
        locations."/" = {
          proxyPass = "http://127.0.0.1:8080";
          extraConfig = ''proxy_set_header Host "127.0.0.1:8081";'';
        };
      };
    };
  };
  testScript = ''
    start_all()
    machine.wait_for_unit("atticd.service")
    machine.wait_for_open_port(8080)
    machine.wait_for_unit("nginx.service")
    print(machine.succeed("python /etc/attic-publication.py"))
  '';
}
