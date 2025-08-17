{
  lib,
  inputs,
  system ? null,
  ...
}: let
  actualSystem =
    if system != null
    then system
    else "x86_64-linux";
  pkgs = inputs.nixpkgs.legacyPackages.${actualSystem};

  # Use a fixed, eval-time path for the flake source (don’t read from a drv output)
  srcPath = builtins.path {
    path = ../../.;
    name = "flake-src";
  };

  lockJson = builtins.fromJSON (builtins.readFile (srcPath + "/flake.lock"));
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

  prefetchedList =
    lib.pipe nodes [(lib.mapAttrsToList prefetchNode) (builtins.filter (x: x != null))];

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
in rec {
  testFlake = pkgs.stdenv.mkDerivation {
    name = "flake-as-git";
    src = srcPath;

    nativeBuildInputs = [pkgs.git];

    # Write directly to $out; keep it simple
    buildPhase = ''
      mkdir -p "$out"
      cp -r "$src"/. "$out/"
      cd "$out"
      git init -q
      git config user.name "Nix Build"
      git config user.email "nix@build.local"
      git add -A
      git commit -q -m "Packaged flake for testing"
    '';

    installPhase = "true";
  };

  inherit lockJson nodes prefetchedList prefetchedPaths registryEntries;
}
