# Regression check for the host-side web-ui-test runner.
#
# The runner executes the existing browser harness against the persistent
# development stack. This check proves the contract the runner adds around
# that harness: workflow selection, rejection of workflows that need the
# NixOS VM, development-stack readiness reporting, artifact creation, and
# exit-status propagation. It starts no services and no virtual machine.
{ pkgs, ... }:
pkgs.runCommand "web-ui-test-runner-check"
{
  nativeBuildInputs = with pkgs; [ bash coreutils gnugrep gnused nodejs ];
} ''
  bash ${../web-ui/tests/web-ui-test-runner-test.sh} \
    ${../web-ui/tests/web-ui-test.sh} \
    ${../web-ui/coverage-manifest.json}
  touch "$out"
''
