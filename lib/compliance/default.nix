# Generic Crystal Forge compliance module helper.
#
# The implementation lives with the generator crate so that a single file is
# both the repository library and the `lib.nix` embedded in every generated
# artifact. That keeps the deployed helper and the tested helper identical.
#
# See `checks/compliance-module` for its unit tests, and
# `docs/operator/nixos-module-generation.md` for consumer usage.
#
# Note: `lib/stig` remains for existing STIG-specific callers. New work should
# use `mkComplianceModule`, which is framework-neutral: a Crystal Forge policy
# may map to DISA, NIST, CIS, or CMMC requirements while keeping one
# implementation representation.
{...}: {
  mkComplianceModule =
    import ../../packages/default/crates/cf-nixos-module/nix/mk-compliance-module.nix;
}
