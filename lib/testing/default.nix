{
  lib,
  inputs,
  system,
  ...
}: let
  pkgs = inputs.nixpkgs.legacyPackages.${system};
in rec {
  testFlake = pkgs.stdenv.mkDerivation {
    name = "flake-as-git";

    # Copy the current directory (but be careful about .git)
    src = ../../.;

    nativeBuildInputs = with pkgs; [git];

    buildPhase = ''
      # Create output directory
      mkdir -p $out

      # Copy all files from source
      cp -r . $out/

      # Initialize git repo in the output
      cd $out
      git init
      git config user.name "Nix Build"
      git config user.email "nix@build.local"

      # Add all files and commit
      git add .
      git commit -m "Packaged flake for testing"
    '';

    # No install phase needed since we build directly into $out
    dontInstall = true;
  };
  lockJson = builtins.fromJSON (builtins.readFile "${testFlake}/flake.lock");
  nodes = lockJson.nodes;

  prefetchNode = name: node: let
    l = node.locked or {};
  in
    if (l.type or "") == "github"
    then {
      key = "github:${l.owner}/${l.repo}";
      path = builtins.fetchTree {
        type = "github";
        owner = l.owner;
        repo = l.repo;
        rev = l.rev;
        narHash = l.narHash;
      };
      from = {
        type = "github";
        owner = l.owner;
        repo = l.repo;
      };
    }
    else if (l.type or "") == "git"
    then {
      key = "git:${l.url}";
      path = builtins.fetchTree {
        type = "git";
        url = l.url;
        rev = l.rev;
        narHash = l.narHash;
      };
      from = {
        type = "git";
        url = l.url;
      };
    }
    else if (l.type or "") == "tarball"
    then {
      key = "tarball:${l.url}";
      path = builtins.fetchTree {
        type = "tarball";
        url = l.url;
        narHash = l.narHash;
      };
      from = {
        type = "tarball";
        url = l.url;
      };
    }
    else null;

  prefetchedList = lib.pipe nodes [
    (lib.mapAttrsToList prefetchNode)
    (builtins.filter (x: x != null))
  ];

  prefetchedPaths = map (x: x.path) prefetchedList;

  registryEntries = lib.listToAttrs (map
    (x:
      lib.nameValuePair
      (builtins.replaceStrings [":" "/" "."] ["-" "-" "-"] x.key)
      {
        from = x.from;
        to = {
          type = "path";
          path = x.path;
        };
      })
    prefetchedList);
}
