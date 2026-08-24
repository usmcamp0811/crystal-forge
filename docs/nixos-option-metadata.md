# NixOS Option Metadata Authority

## Architectural invariant

> **Crystal Forge's packaged NixOS option metadata is an authoring aid derived from CF's pinned nixpkgs. It is not authoritative for a monitored foreign flake. The target flake's own evaluation remains authoritative.**

This distinction is a design invariant for policy authoring, validation, and execution. Packaged metadata can improve the editing experience and identify obvious baseline type or enum mistakes, but it cannot prove whether an option exists or is valid in a target system's module graph.

In particular:

```text
metadata-known      != option guaranteed valid on target
metadata-unknown    != option invalid on target
```

Server and UI changes must preserve both sides of this invariant.

## Source of the packaged catalog

Crystal Forge generates its default option catalog reproducibly from the nixpkgs revision pinned by the Crystal Forge flake:

```text
Crystal Forge pinned nixpkgs
        |
        v
NixOS module option evaluation
        |
        v
generated option metadata
        |
        v
CF policy editor
```

The resulting artifact is packaged with Crystal Forge. It is an internally consistent baseline for that pinned NixOS module graph; the server does not run Nix or access the network to answer editor searches at runtime.

## Policy-authoring intelligence

Crystal Forge uses the catalog to make policy authoring faster and more precise. For example:

```text
services.openssh.enable  -> boolean editor
an enum-valued option    -> dropdown with known values
an integer-valued option -> numeric editor
a string/lines option    -> short or multiline text editor
```

This guidance helps construct a typed semantic rule:

```json
{
  "kind": "nixos_option",
  "config": {
    "path": "services.openssh.enable",
    "operator": "==",
    "value_type": "boolean",
    "value": true
  }
}
```

The catalog does not enforce that rule. It provides autocomplete, editor selection, enum choices, early input guidance, and typed serialization without evaluating a target flake for every editor keystroke.

This baseline is expected to describe many ordinary NixOS options used by targets on the same or nearby nixpkgs revisions. It therefore provides useful defaults and a substantially better authoring experience, but no exact compatibility percentage is guaranteed.

## Foreign flakes can have different schemas

Crystal Forge's packaged metadata describes the option set available from Crystal Forge's pinned nixpkgs. It is not authoritative for the complete module graph of an arbitrary monitored flake.

A monitored flake can use:

- a different, older, or newer nixpkgs revision;
- renamed or removed options;
- changed option types or enum members;
- custom or organization-specific NixOS modules;
- third-party modules such as Home Manager, sops-nix, or disko, where they participate in the evaluated graph.

For example, a target can define an option that is absent from Crystal Forge's baseline:

```nix
options.acme.security.fips.enable = lib.mkOption {
  type = lib.types.bool;
};
```

The packaged catalog cannot know this option from a vanilla evaluation of CF's own pinned NixOS modules. Its absence from search results is not evidence that the target option is invalid.

Version skew also works in both directions. For example:

```text
Crystal Forge baseline:
  nixpkgs release-26.05
  foo.mode enum = ["a", "b", "c"]

Target:
  older nixpkgs
  foo.mode enum = ["a", "b"]
```

The editor may offer `"c"` from the baseline, while the target evaluation rejects it. Conversely, a newer nixpkgs or custom target module can define option `X` even when the baseline does not know `X`.

## Authority hierarchy

The authority boundary is:

```text
CF packaged option metadata
        |
        v
authoring assistance / best-known baseline schema

Target flake evaluation
        |
        v
authoritative truth for that target
```

Only evaluation of the target flake and its actual module composition determines whether:

- an option exists;
- a value and type are accepted;
- all composed modules are valid together; and
- the target configuration evaluates successfully.

Baseline metadata must therefore never be described or consumed as proof that an option exists on a target. A known baseline option can still be absent or different on a target, and an unknown baseline option can still be valid there.

## Unknown/custom fallback

The unknown/custom path is an architectural requirement, not merely error recovery. If the baseline does not recognize `acme.security.fips.enable`, the result means:

```text
Metadata for this option is not available from Crystal Forge's baseline catalog.
The option may still be valid for the target system.
```

The operator can continue authoring the rule with the `unknown` semantic-string representation. A missing, unavailable, or corrupt baseline catalog must not be silently converted into a claim that the option does not exist.

Baseline validation may reject an obvious mismatch against a known baseline entry or require the explicit unknown/custom representation when CF cannot provide a reliable type. That is a serialization and authoring-safety boundary, not a target-validity verdict. The eventual target evaluation remains authoritative.

Tests must preserve this distinction. Do not add a test that treats absence from packaged metadata as proof that a target option is invalid.

## Phase 3 and Phase 4 separation

Phase 3 uses metadata to author and serialize typed semantic rules. Phase 4 must apply those rules to the real target evaluation:

```text
Phase 3:
metadata -> author typed semantic rule

Phase 4:
typed semantic rule + actual target evaluation -> enforcement result
```

Phase 4 must not fail a policy solely because the packaged baseline did not recognize its option, and it must not treat a baseline-known option as proof of target compliance. Execution must obtain its authoritative answer from the actual target configuration evaluation.

## Future target-specific metadata

Target-specific option metadata is a possible future enhancement, not part of Phase 3:

```text
                         CF pinned metadata
                         fast baseline
                              |
                              v
Policy editor ----------------------------+
                                          |
                                          v
                         target-specific metadata
                         from actual flake evaluation
                                          |
                                          v
                         authoritative target schema
```

Such metadata could include the target's exact nixpkgs revision, custom and third-party modules, option types, and enum values. Its generation and transport require a separate design and implementation.

When target-specific metadata is added, cache it against an immutable or reproducible identity of the evaluated target. Candidate identity inputs include the flake lock/input identity, resolved nixpkgs revision, source revision, and module or evaluation identity already tracked by Crystal Forge. This document does not define the final cache-key contract; it records the principle that unchanged immutable evaluation inputs must not cause repeated metadata generation.
