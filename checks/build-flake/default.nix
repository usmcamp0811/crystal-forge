{
  pkgs,
  system,
  ...
}: let
  # Pre-build the hello package on the host so it's available in the VM
  helloPackage = pkgs.writeShellApplication {
    name = "hello";
    text = "echo hello-from-flake\n";
  };

  # Create a flake that just references the pre-built package
  toyFlakeDir = pkgs.runCommand "toyflake-dir" {} ''
    mkdir -p $out
    
    # Write the flake.nix that references the pre-built package
    cat > $out/flake.nix << 'EOF'
    {
      inputs = {};
      outputs = { self }:
      let
        system = "x86_64-linux";
      in {
        packages.${system}.hello = ${helloPackage};
        defaultPackage.${system} = self.packages.${system}.hello;
      };
    }
    EOF
    
    # Create an empty flake.lock
    cat > $out/flake.lock << 'EOF'
    {
      "nodes": {
        "root": {
          "inputs": {},
          "locked": {
            "lastModified": 1,
            "narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "type": "file"
          }
        }
      },
      "root": "root",
      "version": 7
    }
    EOF
  '';
in
  pkgs.testers.runNixOSTest {
    name = "build-inline-flake-in-vm";
    nodes.builder = {pkgs, ...}: {
      services.getty.autologinUser = "root";
      nix = {
        package = pkgs.nixVersions.stable;
        settings.experimental-features = ["nix-command" "flakes"];
        extraOptions = ''
          accept-flake-config = true
          flake-registry = ${pkgs.writeText "empty-registry.json" ''{"flakes":[]}''}
        '';
      };
      environment.etc."toyflake".source = toyFlakeDir;
    };
    testScript = ''
      builder.start()
      builder.wait_for_unit("multi-user.target")
      builder.succeed("nix --version")
      builder.succeed("ls -la /etc/toyflake")
      builder.succeed("cat /etc/toyflake/flake.nix")  # Debug: check flake content
      builder.succeed("nix build /etc/toyflake#hello -o /root/result --impure")
      builder.succeed("/root/result/bin/hello | grep hello-from-flake")
    '';
  }
