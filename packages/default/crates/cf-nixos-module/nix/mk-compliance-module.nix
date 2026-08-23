# Generic Crystal Forge compliance module factory.
#
# This is a *deployment mechanism only*. Crystal Forge is the source of truth
# for policy semantics: by the time a manifest reaches this function, the Rust
# generator has already validated the export, verified immutable identities and
# semantic digests, resolved bundle membership and policy lineages, rejected
# unsupported policies, and detected conflicting assignments.
#
# This function therefore does nothing more than turn
#
#     path (list of strings) + typed value  ->  NixOS option definition
#
# It never parses assertions, never resolves policy versions, and never
# evaluates Nix source contained in the manifest. Manifest values are ordinary
# JSON data; a string stays a string.
#
# Arguments:
#   lib       nixpkgs lib
#   config    the NixOS module `config` argument
#   manifest  the decoded manifest.json (data only)
#   baseline  Nix-safe identifier for this compliance baseline
#
# Result: a NixOS module exposing
#   crystal-forge.compliance.<baseline>.enable
#   crystal-forge.compliance.<baseline>.summary   (read-only provenance)
#
# Importing the module applies the baseline by default. An operator may set
# `enable = false` to disable it explicitly.
#
# Deliberate non-features:
#   * No per-policy enable switches. Enabling a baseline applies every policy
#     the resolved export selected. Exceptions and waivers belong in Crystal
#     Forge's compliance model, not in generated Nix.
#   * No justification tracking. Crystal Forge owns policies, immutable
#     versions, bundles, mappings, resolution, and provenance.
#   * No mkForce / mkOverride. Definitions are ordinary NixOS definitions, so a
#     local configuration that contradicts the baseline produces a normal
#     conflicting-definition error instead of being silently overridden.
{
  lib,
  config,
  manifest,
  baseline,
}: let
  cfg = config.crystal-forge.compliance.${baseline};

  policies = manifest.policies or [];

  # One implementation per policy. A policy that maps to several compliance
  # requirements (DISA, NIST, CIS, ...) still contributes its assignments once;
  # mappings are metadata carried in the manifest.
  assignments = lib.concatMap (policy: policy.assignments or []) policies;

  validPath = assignment:
    assignment ? path
    && builtins.isList assignment.path
    && assignment.path != []
    && builtins.all builtins.isString assignment.path;

  toDefinition = assignment:
    lib.throwIf (!validPath assignment)
    "crystal-forge.compliance.${baseline}: manifest assignment has an invalid option path"
    (lib.setAttrByPath assignment.path assignment.value);
in {
  options.crystal-forge.compliance.${baseline} = {
    enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Whether to apply the Crystal Forge compliance baseline '${baseline}' generated from an immutable Crystal Forge export";
    };

    summary = lib.mkOption {
      type = lib.types.attrs;
      readOnly = true;
      internal = false;
      description = ''
        Read-only provenance for this generated baseline, taken verbatim from
        manifest.json. Present so an operator can confirm exactly which
        immutable Crystal Forge content produced this configuration.
      '';
      default = {
        generator = manifest.generator or null;
        formatVersion = manifest.format_version or null;
        policyCount = builtins.length policies;
        assignmentCount = builtins.length assignments;
        policyVersionIds = map (policy: policy.policy_version_id or null) policies;
        bundles =
          map (bundle: {
            bundleId = bundle.bundle_id or null;
            bundleVersionId = bundle.bundle_version_id or null;
            semanticDigest = bundle.semantic_digest or null;
          })
          (manifest.bundles or []);
        skippedPolicyCount = builtins.length (manifest.skipped_policies or []);
      };
    };
  };

  # Ordinary definitions, applied unless the operator explicitly disables the
  # baseline.
  config = lib.mkIf cfg.enable (lib.mkMerge (map toDefinition assignments));
}
