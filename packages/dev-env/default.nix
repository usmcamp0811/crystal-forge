{
  lib,
  pkgs,
  system,
  inputs,
  ...
}: let
  # Import your dev composition directly - this returns a derivation
  devTest = import ./composition.nix {
    inherit lib inputs pkgs;
  };

  # Create a development VM runner
  devScript = pkgs.writeShellApplication {
    name = "crystal-forge-dev";
    runtimeInputs = with pkgs; [
      qemu
      socat
      vde2
    ];
    text = ''
      echo "Starting Crystal Forge Development Environment..."
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
in
  devScript
  // {
    # Expose the test components for debugging/inspection
    inherit devTest;
    test = devTest;
    driver = devTest.driver;
    driverInteractive = devTest.driverInteractive;
  }
