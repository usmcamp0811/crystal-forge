{
  inputs,
  pkgs,
  ...
}:
pkgs.writeShellApplication {
  name = "web-ui-producer";
  runtimeInputs = [
    pkgs.bash
    pkgs.coreutils
    pkgs.curl
    pkgs.jq
    pkgs.nix
  ];
  text = ''
    exec bash ${inputs.self}/ci/web-ui-producer.sh "$@"
  '';
}
