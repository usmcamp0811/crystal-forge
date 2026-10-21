{pkgs, lib, sourceNonce, ...}: {
  imports = [./target-baseline.nix ./target-network.nix ./network-isolation.nix];
  # The /etc marker proves activation, not merely a successfully built output.
  # Runtime-authored nonces prevent immutable preparation supplying candidates.
  environment.etc."cf-it-revision".text = sourceNonce;
  # Hold the real switch before services/network change so the browser can
  # inspect the actual Started observation. Release is a scenario input, not
  # synthetic completion. A missing release fails the real switch after 180s.
  system.preSwitchChecks.cf-it-controlled-activation = ''
    if [ "$2" = switch ]; then
      deadline=$((SECONDS + 180))
      until ${pkgs.gnugrep}/bin/grep -Fxq ${lib.escapeShellArg (lib.removeSuffix "\n" sourceNonce)} /run/cf-it-allow-activation 2>/dev/null; do
        if [ "$SECONDS" -ge "$deadline" ]; then
          printf '%s\n' 'CF integration activation release timed out' >&2
          exit 1
        fi
        ${pkgs.coreutils}/bin/sleep 1
      done
    fi
  '';
  system.systemBuilderCommands = ''
    printf '%s' ${lib.escapeShellArg sourceNonce} > "$out/cf-it-source-nonce"
  '';
}
