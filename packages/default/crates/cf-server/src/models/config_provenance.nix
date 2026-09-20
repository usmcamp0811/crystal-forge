{ flake, configuration, provenanceLib }:

let
  lib = provenanceLib { inherit flake configuration; };
in
configuration.config.system.build.toplevel // {
  meta = {
    crystalForgeProvenance = lib.provenance;
  };
}
