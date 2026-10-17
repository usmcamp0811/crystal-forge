{lib, ...}: {
  # Supplies pinned packages, browser facts, and NixOS scenario helpers.
  # Caller-supplied pkgs is the Snowfall overlay channel, not a fresh import.
  makeIntegrationTestHarness = {pkgs, inputs}:
    import ./harness.nix {inherit pkgs lib inputs;};
}
