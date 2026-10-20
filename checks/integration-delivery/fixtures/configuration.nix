{lib, sourceNonce, ...}: {
  imports = [./target-baseline.nix ./target-network.nix ./network-isolation.nix];
  # The /etc marker proves activation, not merely a successfully built output.
  # Runtime-authored nonces prevent immutable preparation supplying candidates.
  environment.etc."cf-it-revision".text = sourceNonce;
  system.systemBuilderCommands = ''
    printf '%s' ${lib.escapeShellArg sourceNonce} > "$out/cf-it-source-nonce"
  '';
}
