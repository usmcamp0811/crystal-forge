{
  lib,
  pkgs,
  system,
  inputs,
  ...
}: let
  # Use nixos-compose to build the composition
  composed = inputs.nixos-compose.lib.compose {
    inherit system;
    nixpkgs = inputs.nixpkgs;
    composition = ./composition.nix;
    extraConfigurations = [
      inputs.self.nixosModules.crystal-forge
    ];
  };

  # Get the composition info JSON
  vmComposition = composed."composition::vm";

  composeInfo = lib.importJSON vmComposition;
  qemuScriptPath = composeInfo.all.qemu_script;
in
  # Create a wrapper script that uses nxc to start the VM
  pkgs.writeShellApplication {
    name = "dev-env-vm";
    runtimeInputs = [
      pkgs.vde2
      pkgs.nxc
      pkgs.openssh # for ssh-keygen
    ];
    text = ''
      # Create SSH key if needed
      mkdir -p ~/.ssh
      if [ ! -f ~/.ssh/id_rsa ]; then
        ssh-keygen -t rsa -b 2048 -f ~/.ssh/id_rsa -N ""
      fi

      # Create minimal working directory
      WORK_DIR=$(mktemp -d)
      cd "$WORK_DIR"

      # Copy composition
      cp ${./composition.nix} composition.nix

      # Create nxc.json
      cat > nxc.json << 'EOF'
      {
        "composition": "composition.nix",
        "default_flavour": "vm"
      }
      EOF

      # Create the build directory with symlink
      mkdir -p build
      ln -sf ${vmComposition} build/composition::vm

      # Now nxc start works!
      exec nxc start --interactive
    '';
  }
  // {
    inherit composed vmComposition;
    nxc = pkgs.nxc;
  }
