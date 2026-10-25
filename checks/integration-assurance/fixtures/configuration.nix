{ pkgs, lib, sourceNonce, ... }: {
  imports = [./target-baseline.nix ./target-network.nix ./network-isolation.nix];
  environment.etc."cf-it-revision".text = sourceNonce;
  system.systemBuilderCommands = ''
    printf '%s' ${lib.escapeShellArg sourceNonce} > "$out/cf-it-source-nonce"
  '';
}
