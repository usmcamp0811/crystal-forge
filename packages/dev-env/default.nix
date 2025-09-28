{
  lib,
  pkgs,
  system,
  inputs,
  ...
}: let
  # Your original working VM test
  devTest = import ./composition.nix {
    inherit lib inputs pkgs;
  };

  # Simple nixos-compose composition for Docker (much simpler than your test)
  dockerComposition = nxcArgs: {
    roles = {
      server = {
        pkgs,
        lib,
        ...
      }: {
        # Override conflicting settings from nixos-compose
        security.polkit.enable = lib.mkForce true;

        services.postgresql = {
          enable = true;
          authentication = lib.mkForce "local all all trust";
          initialScript = pkgs.writeText "init.sql" ''
            CREATE USER crystal_forge;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
          '';
        };

        services.crystal-forge = {
          enable = true;
          local-database = true;
          log_level = "debug";

          server = {
            enable = true;
            port = 3000;
            host = "0.0.0.0";
          };
        };

        environment.systemPackages = with pkgs; [
          postgresql
          curl
          jq
          git
          vim
        ];
      };
    };

    testScript = ''
      server.wait_for_unit("postgresql.service")
      server.wait_for_unit("crystal-forge-server.service")
    '';
  };

  # Build Docker version with nixos-compose
  dockerComposed = inputs.nixos-compose.lib.compose {
    inherit pkgs system;
    nixpkgs = inputs.nixpkgs;
    flavour = "docker";
    composition = dockerComposition;
    extraConfigurations = [
      inputs.self.nixosModules.crystal-forge
    ];
  };

  # VM development script (your original working version)
  vmDevScript = pkgs.writeShellApplication {
    name = "crystal-forge-dev-vm";
    runtimeInputs = with pkgs; [
      qemu
      socat
      vde2
    ];
    text = ''
      echo "Starting Crystal Forge VM Development Environment..."
      echo ""
      echo "This will start:"
      echo "  - Crystal Forge server with PostgreSQL"
      echo "  - Git server with test repositories"
      echo "  - Interactive Python shell for VM control"
      echo ""
      echo "Once started, you'll have access to:"
      echo "  - Crystal Forge API at http://localhost:3000"
      echo "  - PostgreSQL at localhost:5433"
      echo "  - Git server at http://localhost:8080"
      echo ""
      echo "Use server.shell_interact() to get a shell on the main server"
      echo ""

      # Run the test with interactive driver
      ${devTest.driverInteractive}/bin/nixos-test-driver
    '';
  };

  # Docker development script using nixos-compose
  dockerDevScript = pkgs.writeShellApplication {
    name = "crystal-forge-dev-docker";
    runtimeInputs = with pkgs; [
      nxc
      docker
      docker-compose
    ];
    text = ''
      echo "Starting Crystal Forge Docker Development Environment..."
      echo ""
      echo "This will start Crystal Forge in Docker containers"
      echo "Access: http://localhost:3000"
      echo ""

      # Create minimal nxc environment
      WORK_DIR=$(mktemp -d)
      cd "$WORK_DIR"

      # Create nxc.json
      cat > nxc.json << 'EOF'
      {
        "composition": "composition.nix",
        "default_flavour": "docker"
      }
      EOF

      # Create build directory with symlink
      mkdir -p build
      ln -sf ${dockerComposed."composition::docker"} build/composition::docker

      echo "Starting with nixos-compose Docker..."
      nxc start --interactive
    '';
  };

  # Main script to choose between VM and Docker
  devScript = pkgs.writeShellApplication {
    name = "crystal-forge-dev";
    text = ''
      echo "Crystal Forge Development Environment"
      echo "======================================"
      echo ""
      echo "Choose your development environment:"
      echo "  1) VMs (full test environment with git server)"
      echo "  2) Docker (lightweight, just Crystal Forge + DB)"
      echo ""
      read -p "Enter choice (1 or 2): " choice

      case $choice in
        1)
          echo "Starting VM environment..."
          ${vmDevScript}/bin/crystal-forge-dev-vm
          ;;
        2)
          echo "Starting Docker environment..."
          ${dockerDevScript}/bin/crystal-forge-dev-docker
          ;;
        *)
          echo "Invalid choice. Please run again and select 1 or 2."
          exit 1
          ;;
      esac
    '';
  };

  # Helper function to create nxc workspace commands (Docker only)
  makeNxcCommand = command:
    pkgs.writeShellApplication {
      name = "crystal-forge-docker-${command}";
      runtimeInputs = with pkgs; [nxc docker docker-compose];
      text = ''
        # Create minimal nxc environment
        WORK_DIR=$(mktemp -d)
        cd "$WORK_DIR"

        # Create nxc.json
        cat > nxc.json << 'EOF'
        {
          "composition": "composition.nix",
          "default_flavour": "docker"
        }
        EOF

        # Create build directory with symlink
        mkdir -p build
        ln -sf ${dockerComposed."composition::docker"} build/composition::docker

        # Run the nxc command
        nxc ${command} "$@"

        # Cleanup
        rm -rf "$WORK_DIR"
      '';
    };

  # Docker passthrough commands
  dockerConnect = makeNxcCommand "connect";
  dockerDriver = makeNxcCommand "driver";
  dockerStop = makeNxcCommand "stop";
  dockerClean = makeNxcCommand "clean";
  dockerHelper = makeNxcCommand "helper";

  # VM commands using the test driver directly (no nixos-compose conversion)
  vmConnect = pkgs.writeShellApplication {
    name = "crystal-forge-vm-connect";
    runtimeInputs = with pkgs; [qemu socat vde2];
    text = ''
      echo "VM connect via test driver..."
      echo "Starting VMs and dropping into interactive shell"
      echo "Use: server.shell_interact() or gitserver.shell_interact()"
      ${devTest.driverInteractive}/bin/nixos-test-driver
    '';
  };

  vmDriver = pkgs.writeShellApplication {
    name = "crystal-forge-vm-driver";
    runtimeInputs = with pkgs; [qemu socat vde2];
    text = ''
      echo "VM driver (same as connect for test-based VMs)"
      ${devTest.driverInteractive}/bin/nixos-test-driver
    '';
  };

  vmStop = pkgs.writeShellApplication {
    name = "crystal-forge-vm-stop";
    text = ''
      echo "To stop VMs, exit the interactive driver or use machine.shutdown() commands"
      echo "VM state is automatically cleaned up when driver exits"
    '';
  };

  vmClean = pkgs.writeShellApplication {
    name = "crystal-forge-vm-clean";
    text = ''
      echo "VM state cleanup happens automatically with NixOS tests"
      echo "No manual cleanup needed"
    '';
  };

  vmHelper = pkgs.writeShellApplication {
    name = "crystal-forge-vm-helper";
    text = ''
      echo "Crystal Forge VM Development Commands:"
      echo "======================================"
      echo ""
      echo "Available machine commands (in interactive driver):"
      echo "  start_all()                    - Start all VMs"
      echo "  server.shell_interact()       - Get shell on main server"
      echo "  gitserver.shell_interact()    - Get shell on git server"
      echo "  server.succeed('command')     - Run command on server"
      echo "  server.wait_for_unit('unit')  - Wait for systemd unit"
      echo "  server.forward_port(8080, 80) - Forward host:8080 to vm:80"
      echo ""
      echo "Access URLs (after port forwarding):"
      echo "  Crystal Forge API: http://localhost:3000"
      echo "  PostgreSQL: localhost:5433"
      echo "  Git Server: http://localhost:8080"
    '';
  };
in
  devScript
  // {
    # Expose the different options
    vm =
      vmDevScript
      // {
        connect = vmConnect;
        driver = vmDriver;
        stop = vmStop;
        clean = vmClean;
        helper = vmHelper;
      };
    docker =
      dockerDevScript
      // {
        connect = dockerConnect;
        driver = dockerDriver;
        stop = dockerStop;
        clean = dockerClean;
        helper = dockerHelper;
      };
    # Expose the original test components
    inherit devTest;
    test = devTest;
    driver = devTest.driver;
    driverInteractive = devTest.driverInteractive;
    # Expose the docker composition (removed vmComposed since it was problematic)
    inherit dockerComposed;
  }
