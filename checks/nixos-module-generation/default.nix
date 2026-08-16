{ pkgs, ... }:
let
  generator = pkgs.crystal-forge.default.cf-nixos-module-drv;
  system = pkgs.stdenv.hostPlatform.system;

  # Evaluate the generated directory as an ordinary NixOS module through the
  # real NixOS module system, then emit the resulting option values as JSON.
  #
  # This is the authoritative validation for this check: the generated Nix must
  # parse, type-check against real NixOS option declarations, and produce the
  # exact values the exported Crystal Forge policies asserted. A string
  # comparison of the generated text would not prove any of that.
  evalGenerated = pkgs.writeText "eval-generated.nix" ''
    { modulePath }:
    let
      evaluated = import ${pkgs.path}/nixos/lib/eval-config.nix {
        system = "${system}";
        modules = [ (/. + modulePath) ];
      };
      cfg = evaluated.config;
    in {
      firewall = cfg.networking.firewall.enable;
      permitRootLogin = cfg.services.openssh.settings.PermitRootLogin;
      passwordAuthentication = cfg.services.openssh.settings.PasswordAuthentication;
      timesyncd = cfg.services.timesyncd.enable;
      allowNullPassword = cfg.security.pam.services.sshd.allowNullPassword;
      # This policy version is deselected by the fixture bundle, so the
      # generated module must leave the option at its NixOS default.
      fail2ban = cfg.services.fail2ban.enable;
    }
  '';
in
pkgs.runCommand "cf-nixos-module-generation"
{
  nativeBuildInputs = [ generator pkgs.nix pkgs.jq ];
  meta = {
    description =
      "Generate NixOS modules from Crystal Forge policy and bundle exports, then evaluate them through the NixOS module system";
  };
} ''
  set -euo pipefail

  export HOME="$TMPDIR"
  export NIX_STATE_DIR="$TMPDIR/nix/var"
  mkdir -p "$NIX_STATE_DIR"

  echo "── 1. Produce exported Crystal Forge artifacts ─────────────────────────"
  cf-nixos-module-fixture policy-set   > policy-set.json
  cf-nixos-module-fixture bundle-xccdf > bundle.xml

  echo "── 2. Generate the standalone NixOS module ─────────────────────────────"
  cf-nixos-module \
    --input policy-set.json \
    --input bundle.xml \
    --output generated

  test -f generated/default.nix
  test -f generated/manifest.json
  test -d generated/policies

  echo "── 3. Evaluate the generated module as a real NixOS module ─────────────"
  nix-instantiate \
    --eval --strict --json --readonly-mode \
    --argstr modulePath "$PWD/generated" \
    ${evalGenerated} > evaluated.json

  cat evaluated.json | jq .

  # Values asserted by the exported policies must now be configured values.
  test "$(jq -r '.firewall' evaluated.json)"               = "true"
  test "$(jq -r '.permitRootLogin' evaluated.json)"        = "no"
  test "$(jq -r '.passwordAuthentication' evaluated.json)" = "false"
  test "$(jq -r '.timesyncd' evaluated.json)"              = "true"
  test "$(jq -r '.allowNullPassword' evaluated.json)"      = "false"

  # The bundle version deselects the fail2ban policy, so it must NOT be applied.
  test "$(jq -r '.fail2ban' evaluated.json)" = "false"

  echo "── 4. Unsupported policies are reported, never implemented ─────────────"
  for skipped in require-physical-console-control block-critical-cves audit-rules-present; do
    jq -e --arg name "$skipped" \
      '.skipped_policies[] | select(.name == $name)' generated/manifest.json > /dev/null
    if grep -rq "$skipped" generated/default.nix generated/policies; then
      echo "FAIL: skipped policy $skipped leaked into the generated Nix" >&2
      exit 1
    fi
  done

  # A deselected bundle policy is not "skipped" — it is simply not part of the
  # exported bundle version's membership, so it must be absent entirely.
  if grep -rq "unselected-baseline-policy" generated; then
    echo "FAIL: a deselected policy version appeared in the output" >&2
    exit 1
  fi

  echo "── 5. Manifest records exact immutable identities ──────────────────────"
  test "$(jq -r '.format_version' generated/manifest.json)" = "1"
  test "$(jq -r '.policies | length' generated/manifest.json)" = "4"
  test "$(jq -r '.bundles[0].bundle_version_id' generated/manifest.json)" \
     = "bbbbbbbb-0000-0000-0000-00000000000b"
  # Every generated policy records a full sha-256 semantic digest.
  test "$(jq -r '[.policies[] | select(.semantic_digest | length == 64)] | length' \
     generated/manifest.json)" = "4"

  echo "── 6. Generation is deterministic ──────────────────────────────────────"
  cf-nixos-module --input policy-set.json --input bundle.xml --output generated-again
  diff -r generated generated-again
  # Input order must not change the output either.
  cf-nixos-module --input bundle.xml --input policy-set.json --output generated-reordered
  diff -r generated generated-reordered

  echo "── 7. The generated module is standalone ───────────────────────────────"
  if grep -rqE 'crystal-forge|getFlake|fetchurl|fetchTarball' \
      generated/default.nix generated/policies; then
    echo "FAIL: generated Nix references Crystal Forge or network infrastructure" >&2
    exit 1
  fi

  echo "── 8. CLI modes behave as documented ───────────────────────────────────"
  # --check validates without writing output.
  cf-nixos-module --input policy-set.json --check
  test ! -e ./out-should-not-exist

  # --strict fails when any policy could not be converted.
  if cf-nixos-module --input policy-set.json --check --strict 2>/dev/null; then
    echo "FAIL: --strict should have failed on unconvertible policies" >&2
    exit 1
  fi

  # A bundle whose policies are all convertible passes --strict.
  cf-nixos-module --input bundle.xml --check --strict

  # --single-file emits one combined module that still evaluates.
  cf-nixos-module --input bundle.xml --output generated-single --single-file
  test ! -d generated-single/policies
  nix-instantiate --eval --strict --json --readonly-mode \
    --argstr modulePath "$PWD/generated-single" \
    ${evalGenerated} > evaluated-single.json
  test "$(jq -r '.timesyncd' evaluated-single.json)" = "true"

  echo "── 9. Tampered exports are rejected ────────────────────────────────────"
  sed 's/config.networking.firewall.enable == true/config.networking.firewall.enable == false/' \
    policy-set.json > tampered.json
  if cf-nixos-module --input tampered.json --check 2>/dev/null; then
    echo "FAIL: a tampered export must fail semantic digest verification" >&2
    exit 1
  fi

  echo "── 10. Conflicting implementations are reported, not resolved ──────────"
  # Two distinct policy versions that configure the same NixOS option with
  # different values. semantic_digest is omitted, so this exercises the
  # cross-policy conflict check rather than digest verification.
  cat > conflicting.json <<'JSON'
  {
    "schema": "urn:crystal-forge:policy-set:1",
    "version": "1",
    "policies": [
      {
        "lineage_id": "11111111-0000-0000-0000-0000000000e1",
        "version_id": "22222222-0000-0000-0000-0000000000e1",
        "name": "firewall-on",
        "policy_type": "custom_check",
        "implementation_state": "native",
        "config": { "expression": "config.networking.firewall.enable == true" }
      },
      {
        "lineage_id": "11111111-0000-0000-0000-0000000000e2",
        "version_id": "22222222-0000-0000-0000-0000000000e2",
        "name": "firewall-off",
        "policy_type": "custom_check",
        "implementation_state": "native",
        "config": { "expression": "config.networking.firewall.enable == false" }
      }
    ]
  }
  JSON

  # Capture the diagnostic before asserting on it: the command is expected to
  # fail, so it must not run inside a pipeline under `set -o pipefail`.
  if cf-nixos-module --input conflicting.json --check > conflict-output.txt 2>&1; then
    echo "FAIL: conflicting option values must be reported" >&2
    exit 1
  fi
  cat conflict-output.txt
  grep -q "networking.firewall.enable" conflict-output.txt
  grep -q "firewall-on"  conflict-output.txt
  grep -q "firewall-off" conflict-output.txt

  echo "── 11. Conflicting definitions of one identity are rejected ────────────"
  # The same immutable version_id defined with two different configurations.
  cat > identity-conflict.json <<'JSON'
  {
    "schema": "urn:crystal-forge:policy-set:1",
    "version": "1",
    "policies": [
      {
        "lineage_id": "11111111-0000-0000-0000-0000000000f1",
        "version_id": "22222222-0000-0000-0000-0000000000f1",
        "name": "same-identity",
        "policy_type": "custom_check",
        "implementation_state": "native",
        "config": { "expression": "config.networking.firewall.enable == true" }
      }
    ]
  }
  JSON
  sed 's/enable == true/enable == false/' identity-conflict.json > identity-conflict-b.json

  if cf-nixos-module --input identity-conflict.json --input identity-conflict-b.json \
       --check > identity-output.txt 2>&1; then
    echo "FAIL: two definitions of one immutable identity must be rejected" >&2
    exit 1
  fi
  cat identity-output.txt
  grep -q "22222222-0000-0000-0000-0000000000f1" identity-output.txt

  echo "── All NixOS module generation checks passed ───────────────────────────"
  mkdir -p "$out"
  cp -r generated "$out/generated"
  cp evaluated.json "$out/evaluated.json"
''
