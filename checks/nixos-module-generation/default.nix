{
  pkgs,
  lib,
  ...
}: let
  generator = pkgs.crystal-forge.default.cf-nixos-module-drv;
  system = pkgs.stdenv.hostPlatform.system;

  # Evaluate the generated artifact as an ordinary NixOS module through the real
  # NixOS module system, then emit the resulting option values as JSON.
  #
  # This is the authoritative validation for this check: the generated artifact
  # must be importable, type-check against real NixOS option declarations, and
  # produce exactly the values the exported Crystal Forge policies asserted. A
  # string comparison of the generated text would prove none of that.
  #
  # `enable` is a parameter so the same harness proves both that the default
  # baseline applies and that an explicit disable prevents application.
  evalGenerated = pkgs.writeText "eval-generated.nix" ''
    { modulePath, baseline, enable ? null }:
    let
      evaluated = import ${pkgs.path}/nixos/lib/eval-config.nix {
        system = "${system}";
        modules = [ (/. + modulePath) ]
          ++ (if enable == null then [] else [
            { crystal-forge.compliance.''${baseline}.enable = enable; }
          ]);
      };
      cfg = evaluated.config;
    in {
      firewall = cfg.networking.firewall.enable;
      permitRootLogin = cfg.services.openssh.settings.PermitRootLogin;
      passwordAuthentication = cfg.services.openssh.settings.PasswordAuthentication;
      timesyncd = cfg.services.timesyncd.enable;
      # The baseline creates this PAM service entry. When the baseline is
      # disabled the entry must not exist at all, which is reported as null.
      allowNullPassword =
        if cfg.security.pam.services ? sshd
        then cfg.security.pam.services.sshd.allowNullPassword
        else null;
      # Deselected by the fixture bundle: must stay at the NixOS default.
      fail2ban = cfg.services.fail2ban.enable;
      # Read-only provenance surfaced by the generic helper.
      summary = cfg.crystal-forge.compliance.''${baseline}.summary;
    }
  '';

in
  pkgs.runCommand "cf-nixos-module-generation" {
    nativeBuildInputs = [generator pkgs.nix pkgs.jq];
    meta = {
      description = "Generate a compliance artifact from Crystal Forge exports and evaluate it through the NixOS module system";
    };
  } ''
    set -euo pipefail

    export HOME="$TMPDIR"
    export NIX_STATE_DIR="$TMPDIR/nix/var"
    mkdir -p "$NIX_STATE_DIR"

    echo "── 1. Produce exported Crystal Forge artifacts ─────────────────────────"
    cf-nixos-module-fixture policy-set   > policy-set.json
    cf-nixos-module-fixture bundle-xccdf > bundle.xml

    echo "── 2. Generate the compliance artifact ─────────────────────────────────"
    cf-nixos-module \
      --input policy-set.json \
      --input bundle.xml \
      --baseline production-baseline \
      --output generated

    echo "── 3. The artifact is minimal: no per-policy Nix files ─────────────────"
    find generated -type f | sort
    test "$(find generated -type f | wc -l)" = "3"
    test -f generated/default.nix
    test -f generated/lib.nix
    test -f generated/manifest.json
    test ! -d generated/policies

    echo "── 4. Assignments are typed JSON data, not Nix source ──────────────────"
    # A boolean assignment must be a JSON boolean and its path a JSON array.
    jq -e '.policies[]
           | select(.name == "disable-root-ssh-login")
           | .assignments[]
           | select(.path == ["services","openssh","settings","PasswordAuthentication"])
           | select(.value == false)' generated/manifest.json > /dev/null
    jq -e '.policies[]
           | select(.name == "disable-root-ssh-login")
           | .assignments[]
           | select(.path == ["services","openssh","settings","PermitRootLogin"])
           | select(.value == "no")' generated/manifest.json > /dev/null
    # Every assignment path is an array and no value is a stringified Nix literal.
    test "$(jq '[.policies[].assignments[] | select((.path|type) != "array")] | length' \
       generated/manifest.json)" = "0"
    test "$(jq '[.policies[].assignments[] | select(.value == "true" or .value == "false")] | length' \
       generated/manifest.json)" = "0"
    # The legacy Nix-source representation must be gone.
    test "$(jq 'any(.policies[]; has("nixos_options"))' generated/manifest.json)" = "false"

    echo "── 5. The generated Nix never evaluates manifest content ───────────────"
    grep -q 'setAttrByPath' generated/lib.nix
    if grep -qE 'fromJSON|builtins\.exec|import \(' generated/lib.nix; then
      echo "FAIL: lib.nix must not decode or evaluate manifest content" >&2
      exit 1
    fi
    # default.nix decodes the manifest as data exactly once, and imports only
    # the static local library.
    test "$(grep -c 'fromJSON' generated/default.nix)" = "1"
    grep -q 'import ./lib.nix' generated/default.nix
    if grep -qE 'import \(|builtins\.exec' generated/default.nix; then
      echo "FAIL: default.nix must not import a computed path" >&2
      exit 1
    fi
    # No policy value leaks into the generated Nix; all data lives in the manifest.
    if grep -qE 'PermitRootLogin|timesyncd|firewall' generated/default.nix generated/lib.nix; then
      echo "FAIL: policy data leaked into the generated Nix" >&2
      exit 1
    fi

    echo "── 6. Imported without enable setting: baseline applies ────────────────"
    nix-instantiate --eval --strict --json --readonly-mode \
      --argstr modulePath "$PWD/generated" \
      --argstr baseline production-baseline \
       ${evalGenerated} > enabled.json
    jq . enabled.json

    test "$(jq -r '.firewall' enabled.json)"               = "true"
    test "$(jq -r '.permitRootLogin' enabled.json)"        = "no"
    test "$(jq -r '.passwordAuthentication' enabled.json)" = "false"
    test "$(jq -r '.timesyncd' enabled.json)"              = "true"
    test "$(jq -r '.allowNullPassword' enabled.json)"      = "false"
    # The bundle version deselects the fail2ban policy, so it must NOT apply.
    test "$(jq -r '.fail2ban' enabled.json)" = "false"
    # Provenance is visible from the evaluated configuration.
    test "$(jq -r '.summary.policyCount' enabled.json)" = "4"
    test "$(jq -r '.summary.skippedPolicyCount' enabled.json)" = "3"

    echo "── 7. Explicitly disabled: nothing is applied ─────────────────────────"
    nix-instantiate --eval --strict --json --readonly-mode \
      --argstr modulePath "$PWD/generated" \
      --argstr baseline production-baseline \
      --arg enable false \
      ${evalGenerated} > disabled.json
    jq . disabled.json

    # Every asserted option falls back to its NixOS default.
    test "$(jq -r '.firewall' disabled.json)"               = "true"   # NixOS default
    test "$(jq -r '.permitRootLogin' disabled.json)"        = "prohibit-password"
    test "$(jq -r '.passwordAuthentication' disabled.json)" = "true"
    test "$(jq -r '.timesyncd' disabled.json)"              = "true"   # NixOS default
    # The baseline's PAM service entry must not exist at all when disabled.
    test "$(jq -r '.allowNullPassword' disabled.json)"      = "null"

    # The two evaluations must differ, proving the enable option is load-bearing.
    if diff -q enabled.json disabled.json > /dev/null; then
      echo "FAIL: enabling the baseline changed nothing" >&2
      exit 1
    fi

    echo "── 8. Unsupported policies are reported, never implemented ─────────────"
    for skipped in require-physical-console-control block-critical-cves audit-rules-present; do
      jq -e --arg name "$skipped" \
        '.skipped_policies[] | select(.name == $name)' generated/manifest.json > /dev/null
      if jq -e --arg name "$skipped" \
          '.policies[] | select(.name == $name)' generated/manifest.json > /dev/null; then
        echo "FAIL: skipped policy $skipped was implemented" >&2
        exit 1
      fi
    done
    if grep -rq "unselected-baseline-policy" generated; then
      echo "FAIL: a deselected policy version appeared in the output" >&2
      exit 1
    fi

    echo "── 9. Manifest records exact immutable identities ──────────────────────"
    test "$(jq -r '.format_version' generated/manifest.json)" = "2"
    test "$(jq -r '.baseline' generated/manifest.json)" = "production-baseline"
    test "$(jq -r '.policies | length' generated/manifest.json)" = "4"
    test "$(jq -r '.bundles[0].bundle_version_id' generated/manifest.json)" \
       = "bbbbbbbb-0000-0000-0000-00000000000b"
    test "$(jq -r '[.policies[] | select(.semantic_digest | length == 64)] | length' \
       generated/manifest.json)" = "4"

    echo "── 10. Deterministic generation, including reordered inputs ────────────"
    cf-nixos-module --input policy-set.json --input bundle.xml \
      --baseline production-baseline --output generated-again
    diff -r generated generated-again

    cf-nixos-module --input bundle.xml --input policy-set.json \
      --baseline production-baseline --output generated-reordered
    diff -r generated generated-reordered

    # The same immutable content supplied twice under different file names must
    # also produce byte-identical output in either order.
    cp policy-set.json copy-a.json
    cp policy-set.json copy-b.json
    cf-nixos-module --input copy-a.json --input copy-b.json \
      --baseline dup --output dup-forward
    cf-nixos-module --input copy-b.json --input copy-a.json \
      --baseline dup --output dup-reverse
    diff -r dup-forward dup-reverse

    echo "── 11. The artifact is standalone ──────────────────────────────────────"
    if grep -rqE 'crystal-forge-agent|getFlake|fetchurl|fetchTarball' \
        generated/default.nix generated/lib.nix; then
      echo "FAIL: generated Nix references Crystal Forge or network infrastructure" >&2
      exit 1
    fi

    echo "── 12. No per-policy enable switches and no Nix-side justification ─────"
    # Inspect code only; the library's comments legitimately explain why these
    # features are deliberately absent.
    grep -hv '^[[:space:]]*#' generated/default.nix generated/lib.nix > generated-code.nix
    if grep -qE 'policies\.[a-zA-Z0-9_-]+\.enable' generated-code.nix; then
      echo "FAIL: per-policy enable switches must not exist" >&2
      exit 1
    fi
    if grep -qiE 'justification|waiver|exception' generated-code.nix; then
      echo "FAIL: generated Nix must not implement a justification model" >&2
      exit 1
    fi
    # Exactly one enable option is declared, for the baseline itself.
    test "$(grep -c 'enable = lib.mkOption' generated-code.nix)" = "1"

    echo "── 13. CLI modes behave as documented ──────────────────────────────────"
    cf-nixos-module --input policy-set.json --check
    if cf-nixos-module --input policy-set.json --check --strict 2>/dev/null; then
      echo "FAIL: --strict should have failed on unconvertible policies" >&2
      exit 1
    fi
    cf-nixos-module --input bundle.xml --check --strict

    echo "── 14. Tampered exports are rejected ───────────────────────────────────"
    sed 's/config.networking.firewall.enable == true/config.networking.firewall.enable == false/' \
      policy-set.json > tampered.json
    if cf-nixos-module --input tampered.json --check 2>/dev/null; then
      echo "FAIL: a tampered export must fail semantic digest verification" >&2
      exit 1
    fi

    echo "── 15. Conflicting implementations are reported, not resolved ──────────"
    cat > conflicting.json <<'JSON'
    {
      "schema": "urn:crystal-forge:policy-set:1",
      "version": "1",
      "policies": [
        {
          "lineage_id": "11111111-0000-0000-0000-0000000000e1",
          "version_id": "22222222-0000-0000-0000-0000000000e1",
          "publication_state": "accepted",
          "name": "firewall-on",
          "policy_type": "custom_check",
          "implementation_state": "native",
          "config": { "expression": "config.networking.firewall.enable == true" }
        },
        {
          "lineage_id": "11111111-0000-0000-0000-0000000000e2",
          "version_id": "22222222-0000-0000-0000-0000000000e2",
          "publication_state": "accepted",
          "name": "firewall-off",
          "policy_type": "custom_check",
          "implementation_state": "native",
          "config": { "expression": "config.networking.firewall.enable == false" }
        }
      ]
    }
    JSON

    if cf-nixos-module --input conflicting.json --check > conflict-output.txt 2>&1; then
      echo "FAIL: conflicting option values must be reported" >&2
      exit 1
    fi
    cat conflict-output.txt
    grep -q "networking.firewall.enable" conflict-output.txt

    echo "── 16. One lineage may not resolve to two versions ─────────────────────"
    # Same lineage, two different immutable versions, disjoint options. This must
    # be an effective-set conflict, never a silently merged hybrid.
    cat > lineage-a.json <<'JSON'
    {
      "policies": [
        {
          "lineage_id": "11111111-0000-0000-0000-0000000000f0",
          "version_id": "22222222-0000-0000-0000-0000000000f1",
          "publication_state": "accepted",
          "name": "same-lineage",
          "policy_type": "custom_check",
          "implementation_state": "native",
          "config": { "expression": "config.services.openssh.enable == true" }
        }
      ]
    }
    JSON
    cat > lineage-b.json <<'JSON'
    {
      "policies": [
        {
          "lineage_id": "11111111-0000-0000-0000-0000000000f0",
          "version_id": "22222222-0000-0000-0000-0000000000f2",
          "publication_state": "accepted",
          "name": "same-lineage",
          "policy_type": "custom_check",
          "implementation_state": "native",
          "config": { "expression": "config.networking.firewall.enable == true" }
        }
      ]
    }
    JSON

    if cf-nixos-module --input lineage-a.json --input lineage-b.json \
         --check > lineage-output.txt 2>&1; then
      echo "FAIL: two versions of one lineage must not silently coexist" >&2
      exit 1
    fi
    cat lineage-output.txt
    grep -q "CF_EFFECTIVE_POLICY_VERSION_CONFLICT" lineage-output.txt

    echo "── 17. Conflicting definitions of one identity are rejected ────────────"
    sed 's/enable == true/enable == false/' lineage-a.json > lineage-a-modified.json
    if cf-nixos-module --input lineage-a.json --input lineage-a-modified.json \
         --check > identity-output.txt 2>&1; then
      echo "FAIL: one immutable identity must not have two definitions" >&2
      exit 1
    fi
    cat identity-output.txt
    grep -q "CF_POLICY_VERSION_DIGEST_CONFLICT" identity-output.txt

    echo "── All NixOS module generation checks passed ───────────────────────────"
    mkdir -p "$out"
    cp -r generated "$out/generated"
    cp enabled.json disabled.json "$out/"
  ''
