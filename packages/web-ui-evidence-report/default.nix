{
  inputs,
  pkgs,
  ...
}:
pkgs.writeShellApplication {
  name = "web-ui-evidence-report";
  runtimeInputs = [
    pkgs.curl
    pkgs.nodejs
  ];
  text = ''
    exec node ${inputs.self}/ci/web-ui-aggregate.js "$@"
  '';
}
