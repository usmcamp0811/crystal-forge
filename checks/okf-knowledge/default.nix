{ pkgs, lib, ... }:

let
  python = pkgs.python3.withPackages (p: [ p.pyyaml ]);
  # Use the whole repo as source - the check will find docs/knowledge/
  src = ./.;
in
pkgs.runCommand "okf-knowledge-validation" {
  nativeBuildInputs = [ python ];
  meta = {
    description = "Validate the Crystal Forge OKF knowledge bundle in docs/knowledge/";
  };
} ''
  mkdir -p $out
  ${python}/bin/python ${./validate.py} --repo-root ${src} --bundle docs/knowledge
  touch $out/ok
''
