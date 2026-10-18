{ pkgs, lib, ... }:

let
  python = pkgs.python3.withPackages (p: [ p.pyyaml ]);
  src = ../..;
in
pkgs.runCommand "okf-knowledge-validation" {
  nativeBuildInputs = [ python pkgs.nodejs pkgs.mermaid-cli pkgs.git ];
  meta = {
    description = "Validate the Crystal Forge OKF knowledge bundle in docs/knowledge/";
  };
} ''
  mkdir -p $out
  OKF_MERMAID_NODE=${pkgs.nodejs}/bin/node \
  OKF_MERMAID_MODULE=${pkgs.mermaid-cli}/lib/node_modules/@mermaid-js/mermaid-cli/node_modules/mermaid/dist/mermaid.core.mjs \
    ${python}/bin/python -m unittest discover -s ${./.} -p 'test_*.py' -v
  ${python}/bin/python ${./diagram_scan.py} --repo-root ${src} --check --exceptions ${./diagram-audit/exceptions.tsv}
  ${python}/bin/python ${./validate_mermaid_syntax.py} \
    --repo-root ${src} \
    --node ${pkgs.nodejs}/bin/node \
    --mermaid-module ${pkgs.mermaid-cli}/lib/node_modules/@mermaid-js/mermaid-cli/node_modules/mermaid/dist/mermaid.core.mjs \
    --report "$out/mermaid-syntax-report.json"
  ${python}/bin/python ${./validate.py} --repo-root ${src} --bundle docs/knowledge
  touch $out/ok
''
