{ lib, pkgs, inputs, ... }:

# Reuse the shipped embedded-UI gate and its native cache fixtures. Explicit
# selection is pure and drives both fixture startup and browser workflows.
# Baseline updates must never relax this blocking acceptance gate.
import ../web-ui/default.nix {
  inherit lib pkgs inputs;
  testSteps = "25-caches-modal-attic";
  updateVisualBaselines = false;
}
