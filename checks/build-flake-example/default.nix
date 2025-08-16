{
  pkgs,
  system,
  inputs,
  ...
}: let
  lib = pkgs.lib;

  # Pre-build a hello package for the test system
  helloPackage = pkgs.writeShellApplication {
    name = "hello";
    text = "echo hello-from-crystal-forge-test\n";
  };

  # Pre-build a minimal NixOS system for testing
  nixosSystemToplevel =
    (import (pkgs.path + "/nixos/lib/eval-config.nix") {
      inherit system;
      modules = [
        {
          boot.isContainer = true;
          fileSystems."/" = {
            device = "none";
            fsType = "tmpfs";
          };
          services.getty.autologinUser = "root";
          environment.systemPackages = [helloPackage];
          system.stateVersion = "25.05";
          services.udisks2.enable = false;
          security.polkit.enable = false;
          documentation.enable = false;
          documentation.nixos.enable = false;
          system.nssModules = lib.mkForce [];
        }
      ];
    }).config.system.build.toplevel;

  # Create a test flake for Crystal Forge
  testFlakeDir = pkgs.runCommand "crystal-forge-test-flake" {} ''
    mkdir -p $out
    cat > $out/flake.nix << 'EOF'
    {
      inputs = {
        nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
      };
      outputs = { self, nixpkgs }:
      let
        system = "${system}";
        pkgs = import nixpkgs { inherit system; };
      in {
        packages.''${system}.hello = ${helloPackage};
        defaultPackage.''${system} = self.packages.''${system}.hello;

        nixosConfigurations = {
          test-system = nixpkgs.lib.nixosSystem {
            inherit system;
            modules = [
              {
                boot.isContainer = true;
                fileSystems."/" = { device = "none"; fsType = "tmpfs"; };
                services.getty.autologinUser = "root";
                environment.systemPackages = [ self.packages.''${system}.hello ];
                system.stateVersion = "25.05";
                services.udisks2.enable = false;
                security.polkit.enable = false;
                documentation.enable = false;
                documentation.nixos.enable = false;
                system.nssModules = pkgs.lib.mkForce [];
              }
            ];
          };
        };
      };
    }
    EOF

    cat > $out/flake.lock << 'EOF'
    {
      "nodes": {
        "nixpkgs": {
          "locked": {
            "lastModified": 1,
            "narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "type": "github",
            "owner": "NixOS",
            "repo": "nixpkgs"
          },
          "original": {
            "type": "github",
            "owner": "NixOS",
            "repo": "nixpkgs",
            "ref": "nixos-unstable"
          }
        },
        "root": {
          "inputs": {
            "nixpkgs": "nixpkgs"
          }
        }
      },
      "root": "root",
      "version": 7
    }
    EOF
  '';

  # Create a bare git repo containing the test flake
  testFlakeGit = pkgs.runCommand "crystal-forge-test-flake.git" {buildInputs = [pkgs.git];} ''
    set -eu
    export HOME=$PWD
    work="$TMPDIR/w"
    mkdir -p "$work"
    cp -r ${testFlakeDir}/* "$work"/
    chmod -R u+rwX "$work"

    git -C "$work" init
    git -C "$work" config user.name "Crystal Forge Test"
    git -C "$work" config user.email "test@crystal-forge.dev"
    git -C "$work" add .
    GIT_AUTHOR_DATE="1970-01-01T00:00:00Z" \
    GIT_COMMITTER_DATE="1970-01-01T00:00:00Z" \
      git -C "$work" commit -m "Initial test flake commit" --no-gpg-sign

    git init --bare "$out"
    git -C "$out" config receive.denyCurrentBranch ignore
    git -C "$work" push "$out" HEAD:refs/heads/main
    git -C "$out" symbolic-ref HEAD refs/heads/main
  '';

  # Generate test keypair for Crystal Forge
  testKeyPair = pkgs.runCommand "test-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f $out/test.key
  '';
in
  pkgs.testers.runNixOSTest {
    name = "crystal-forge-nixos-build-integration";

    nodes.builder = {
      pkgs,
      config,
      ...
    }: {
      imports = [inputs.self.nixosModules.crystal-forge];

      services.getty.autologinUser = "root";
      virtualisation.writableStore = true;
      virtualisation.memorySize = 4096;
      virtualisation.cores = 2;

      # Enable Crystal Forge with test configuration
      services.crystal-forge = {
        enable = true;
        local-database = true;
        log_level = "debug";

        database = {
          user = "crystal_forge";
          host = "localhost";
          name = "crystal_forge";
        };

        # Configure test flake for watching
        flakes.watched = [
          {
            name = "test-flake";
            repo_url = "/etc/test-flake.git";
            auto_poll = false;
          }
        ];

        # Test environment configuration
        environments = [
          {
            name = "test";
            description = "Test environment for Crystal Forge NixOS builds";
            is_active = true;
            risk_profile = "LOW";
            compliance_level = "NONE";
          }
        ];

        # Test system configuration
        systems = [
          {
            hostname = "test-system";
            public_key = lib.strings.trim (builtins.readFile "${testKeyPair}/test.pub");
            environment = "test";
            flake_name = "test-flake";
          }
        ];

        server = {
          enable = true;
          host = "0.0.0.0";
          port = 3000;
        };
      };

      # PostgreSQL setup
      services.postgresql = {
        enable = true;
        authentication = lib.concatStringsSep "\n" [
          "local all root trust"
          "local all postgres peer"
          "host all all 127.0.0.1/32 trust"
          "host all all ::1/128 trust"
        ];
        initialScript = pkgs.writeText "init-crystal-forge.sql" ''
          CREATE USER crystal_forge LOGIN;
          CREATE DATABASE crystal_forge OWNER crystal_forge;
          GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
        '';
      };

      # Nix configuration for building
      nix = {
        package = pkgs.nixVersions.stable;
        settings = {
          experimental-features = ["nix-command" "flakes"];
          substituters = [];
          builders-use-substitutes = false;
          fallback = false;
          sandbox = true;
        };
        extraOptions = ''
          accept-flake-config = true
          flake-registry = ${pkgs.writeText "empty-registry.json" ''{"flakes":[]}''}
        '';
      };

      nix.nixPath = ["nixpkgs=${pkgs.path}"];
      environment.systemPackages = [pkgs.git pkgs.crystal-forge.cli];

      # Mount test assets
      environment.etc = {
        "test-flake.git".source = testFlakeGit;
        "test.key".source = "${testKeyPair}/test.key";
        "test.pub".source = "${testKeyPair}/test.pub";
      };
    };

    globalTimeout = 900; # 15 minutes for build operations
    extraPythonPackages = p: [p.pytest];

    testScript = ''
            import pytest
            import time

            builder.start()

            # Create output directory and initialize report file
            builder.succeed("mkdir -p /tmp/test-results")
            report_file = "/tmp/test-results/crystal-forge-test-report.txt"

            # Initialize the report file with header
            builder.succeed(f'''
              cat > {report_file} << 'EOF'
      ========================================
      Crystal Forge NixOS Build Integration Test Report
      Generated: $(date)
      ========================================

      EOF
            ''')

            # Wait for PostgreSQL and Crystal Forge services
            builder.wait_for_unit("postgresql")
            builder.wait_for_unit("crystal-forge-server.service")
            builder.wait_for_unit("crystal-forge-builder.service")

            # Verify services are active
            builder.succeed("systemctl is-active postgresql") \
              or pytest.fail("PostgreSQL is not active")
            builder.succeed("systemctl is-active crystal-forge-server.service") \
              or pytest.fail("Crystal Forge server is not active")
            builder.succeed("systemctl is-active crystal-forge-builder.service") \
              or pytest.fail("Crystal Forge builder is not active")

            # Log service status to report
            builder.succeed(f'''
              echo "SERVICE STATUS CHECK:" >> {report_file}
              echo "===================" >> {report_file}
              echo "PostgreSQL: $(systemctl is-active postgresql)" >> {report_file}
              echo "Crystal Forge Server: $(systemctl is-active crystal-forge-server.service)" >> {report_file}
              echo "Crystal Forge Builder: $(systemctl is-active crystal-forge-builder.service)" >> {report_file}
              echo "" >> {report_file}
            ''')

            # Verify Crystal Forge user can access nix
            builder.succeed("sudo -u crystal-forge nix --version") \
              or pytest.fail("crystal-forge user cannot access nix command")

            # Check that the test flake repo is accessible
            builder.succeed("test -d /etc/test-flake.git")
            builder.succeed("ls -la /etc/test-flake.git/")

            # Wait for Crystal Forge to initialize and sync config to database
            builder.wait_until_succeeds("journalctl -u crystal-forge-server.service | grep -i 'server started\\|listening'", timeout=60)

            # Get the commit hash for testing
            commit_hash = builder.succeed("cd /tmp/test-flake && git rev-parse HEAD").strip()
            builder.log(f"Test commit hash: {commit_hash}")

            # Log commit info to report
            builder.succeed(f'''
              echo "TEST SETUP:" >> {report_file}
              echo "==========" >> {report_file}
              echo "Test commit hash: {commit_hash}" >> {report_file}
              echo "Test flake directory accessible: $(test -d /etc/test-flake.git && echo 'YES' || echo 'NO')" >> {report_file}
              echo "" >> {report_file}
            ''')

            # Insert test commit into Crystal Forge database via API or CLI
            builder.log("Adding test commit to Crystal Forge...")

            # Use Crystal Forge CLI to trigger evaluation
            # This should create the necessary database entries and trigger the build loop
            builder.succeed("sudo -u crystal-forge crystal-forge-cli flake add test-flake /etc/test-flake.git")

            # Wait for the build loop to pick up and process the commit
            builder.log("Waiting for Crystal Forge to process the commit...")
            builder.wait_until_succeeds(
              "journalctl -u crystal-forge-builder.service | grep -E '(Starting build|Build completed|test-system)'",
              timeout=300
            )

            # Verify that Crystal Forge successfully built the NixOS system
            builder.log("Verifying NixOS system build...")

            # Check that derivations were created and processed
            builder.wait_until_succeeds(
              "sudo -u crystal-forge psql crystal_forge -c \"SELECT COUNT(*) FROM derivations WHERE derivation_type = 'nixos' AND derivation_name = 'test-system';\" | grep -v '^0$'",
              timeout=60
            )

            # Log database state before checking build status
            builder.succeed(f'''
              echo "DATABASE STATE DURING BUILD:" >> {report_file}
              echo "============================" >> {report_file}
              echo "" >> {report_file}

              echo "Flakes in database:" >> {report_file}
              sudo -u crystal-forge psql crystal_forge -c "\\pset format wrapped" -c "\\pset columns 100" -c "SELECT id, name, repository_url, created_at FROM flakes ORDER BY id;" >> {report_file}
              echo "" >> {report_file}

              echo "Commits in database:" >> {report_file}
              sudo -u crystal-forge psql crystal_forge -c "\\pset format wrapped" -c "\\pset columns 100" -c "SELECT c.id, c.hash, c.message, f.name as flake_name, c.created_at FROM commits c JOIN flakes f ON c.flake_id = f.id ORDER BY c.id;" >> {report_file}
              echo "" >> {report_file}

              echo "Derivations in database:" >> {report_file}
              sudo -u crystal-forge psql crystal_forge -c "\\pset format wrapped" -c "\\pset columns 100" -c "SELECT d.id, d.derivation_name, d.derivation_type, ds.name as status, d.derivation_path, d.created_at FROM derivations d LEFT JOIN derivation_statuses ds ON d.status_id = ds.id ORDER BY d.id;" >> {report_file}
              echo "" >> {report_file}
            ''')

            # Check build status in database
            build_status = builder.succeed(
              "sudo -u crystal-forge psql crystal_forge -t -c \"SELECT ds.name FROM derivations d JOIN derivation_statuses ds ON d.status_id = ds.id WHERE d.derivation_name = 'test-system' ORDER BY d.id DESC LIMIT 1;\""
            ).strip()

            builder.log(f"Build status: {build_status}")

            # Log build results to report
            builder.succeed(f'''
              echo "BUILD RESULTS:" >> {report_file}
              echo "==============" >> {report_file}
              echo "Final build status: {build_status}" >> {report_file}
              echo "" >> {report_file}
            ''')

            # Verify successful build (should be 'build-complete' or similar success status)
            if build_status not in ["build-complete", "completed", "success"]:
              # Get more detailed error information
              error_info = builder.succeed(
                "sudo -u crystal-forge psql crystal_forge -t -c \"SELECT error_message FROM derivations WHERE derivation_name = 'test-system' ORDER BY id DESC LIMIT 1;\""
              ).strip()
              builder.log(f"Build error: {error_info}")

              # Log error details to report
              builder.succeed(f'''
                echo "BUILD ERROR DETAILS:" >> {report_file}
                echo "===================" >> {report_file}
                echo "Error message: {error_info}" >> {report_file}
                echo "" >> {report_file}
                echo "Recent builder logs:" >> {report_file}
                journalctl -u crystal-forge-builder.service --since '5 minutes ago' >> {report_file}
                echo "" >> {report_file}
              ''')

              # Show recent builder logs for debugging
              builder.succeed("journalctl -u crystal-forge-builder.service --since '5 minutes ago'")
              pytest.fail(f"Build did not complete successfully. Status: {build_status}, Error: {error_info}")

            # Verify the built system has the expected components
            builder.log("Verifying built system contents...")

            # Get the store path of the built system
            store_path = builder.succeed(
              "sudo -u crystal-forge psql crystal_forge -t -c \"SELECT derivation_path FROM derivations WHERE derivation_name = 'test-system' AND derivation_path IS NOT NULL ORDER BY id DESC LIMIT 1;\""
            ).strip()

            if store_path:
              builder.log(f"Built system store path: {store_path}")
              builder.succeed(f"test -e {store_path}")
              builder.succeed(f"test -x {store_path}/bin/switch-to-configuration")

              # Verify our test package is included
              builder.succeed(f"find {store_path} -name '*hello*' | grep -q hello")

              # Log store path verification to report
              builder.succeed(f'''
                echo "STORE PATH VERIFICATION:" >> {report_file}
                echo "========================" >> {report_file}
                echo "Store path: {store_path}" >> {report_file}
                echo "Store path exists: $(test -e {store_path} && echo 'YES' || echo 'NO')" >> {report_file}
                echo "switch-to-configuration exists: $(test -x {store_path}/bin/switch-to-configuration && echo 'YES' || echo 'NO')" >> {report_file}
                echo "Test package (hello) found: $(find {store_path} -name '*hello*' | grep -q hello && echo 'YES' || echo 'NO')" >> {report_file}
                echo "" >> {report_file}
              ''')
            else:
              builder.succeed(f'''
                echo "STORE PATH ERROR:" >> {report_file}
                echo "=================" >> {report_file}
                echo "No store path found for built system" >> {report_file}
                echo "" >> {report_file}
              ''')
              pytest.fail("No store path found for built system")

            # Test that Crystal Forge can handle the full workflow
            builder.log("Testing complete Crystal Forge workflow...")

            # Verify CVE scanning capability (should be queued but might not complete in test time)
            scan_count = builder.succeed(
              "sudo -u crystal-forge psql crystal_forge -c \"SELECT COUNT(*) FROM cve_scans WHERE derivation_id IN (SELECT id FROM derivations WHERE derivation_name = 'test-system');\" | tail -n 1"
            ).strip()

            builder.log(f"CVE scans initiated: {scan_count}")

            # Log CVE scan info to report
            builder.succeed(f'''
              echo "CVE SCANNING:" >> {report_file}
              echo "=============" >> {report_file}
              echo "CVE scans initiated: {scan_count}" >> {report_file}
              echo "" >> {report_file}

              echo "CVE scan details:" >> {report_file}
              sudo -u crystal-forge psql crystal_forge -c "\\pset format wrapped" -c "\\pset columns 100" -c "SELECT cs.id, cs.scan_status, cs.vulnerabilities_found, cs.created_at FROM cve_scans cs JOIN derivations d ON cs.derivation_id = d.id WHERE d.derivation_name = 'test-system' ORDER BY cs.id;" >> {report_file}
              echo "" >> {report_file}
            ''')

            # Final verification - check that Crystal Forge maintained system state
            builder.log("Final system verification...")
            builder.succeed("systemctl is-active crystal-forge-server.service")
            builder.succeed("systemctl is-active crystal-forge-builder.service")

            # Verify database integrity and log final state
            builder.succeed(f'''
              echo "FINAL DATABASE STATE:" >> {report_file}
              echo "=====================" >> {report_file}
              echo "" >> {report_file}

              echo "Total flakes: $(sudo -u crystal-forge psql crystal_forge -t -c "SELECT COUNT(*) FROM flakes;" | xargs)" >> {report_file}
              echo "Total commits: $(sudo -u crystal-forge psql crystal_forge -t -c "SELECT COUNT(*) FROM commits;" | xargs)" >> {report_file}
              echo "Total derivations: $(sudo -u crystal-forge psql crystal_forge -t -c "SELECT COUNT(*) FROM derivations;" | xargs)" >> {report_file}
              echo "Total CVE scans: $(sudo -u crystal-forge psql crystal_forge -t -c "SELECT COUNT(*) FROM cve_scans;" | xargs)" >> {report_file}
              echo "" >> {report_file}

              echo "Derivation status breakdown:" >> {report_file}
              sudo -u crystal-forge psql crystal_forge -c "\\pset format wrapped" -c "\\pset columns 100" -c "SELECT ds.name as status, COUNT(*) as count FROM derivations d JOIN derivation_statuses ds ON d.status_id = ds.id GROUP BY ds.name ORDER BY count DESC;" >> {report_file}
              echo "" >> {report_file}
            ''')

            builder.succeed(f'''
              echo "========================================" >> {report_file}
              echo "Test completed successfully at: $(date)" >> {report_file}
              echo "========================================" >> {report_file}
            ''')

            # Copy the report to the proper result directory
            # NixOS tests automatically extract files from /tmp/xchg/
            builder.succeed("mkdir -p /tmp/xchg")
            builder.succeed(f"cp {report_file} /tmp/xchg/crystal-forge-test-report.txt")

            # Also copy any logs that might be useful
            builder.succeed("journalctl -u crystal-forge-server.service > /tmp/xchg/server.log")
            builder.succeed("journalctl -u crystal-forge-builder.service > /tmp/xchg/builder.log")
            builder.succeed("journalctl -u postgresql > /tmp/xchg/postgresql.log")

            builder.log("✅ Crystal Forge NixOS build integration test completed successfully!")
            builder.log(f"📊 Test report saved to: {report_file}")
            builder.log("📁 Reports will be available in the test result output")
    '';
  }
