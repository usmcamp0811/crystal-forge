{
  pkgs,
  lib,
  ...
}:
with lib;
with lib.crystal-forge; let
  # Version management scripts
  bump-patch = pkgs.writeShellScriptBin "bump-patch" ''
    set -euo pipefail

    # Get current version
    CURRENT=$(grep '^version =' packages/default/Cargo.toml | cut -d'"' -f2)
    echo "Current version: $CURRENT"

    # Increment patch version
    major=$(echo $CURRENT | cut -d'.' -f1)
    minor=$(echo $CURRENT | cut -d'.' -f2)
    patch=$(echo $CURRENT | cut -d'.' -f3)
    NEW_VERSION="$major.$minor.$((patch + 1))"
    echo "Bumping to: $NEW_VERSION"

    # Update Cargo.toml
    ${pkgs.gnused}/bin/sed -i "s/^version = \".*\"/version = \"$NEW_VERSION\"/" packages/default/Cargo.toml

    # Commit and push (only if in CI)
    if [ "''${CI:-}" = "true" ]; then
      ${pkgs.git}/bin/git config user.name "Crystal Forge CI"
      ${pkgs.git}/bin/git config user.email "ci@crystal-forge"
      ${pkgs.git}/bin/git add packages/default/Cargo.toml
      ${pkgs.git}/bin/git commit -m "chore: auto-bump version to $NEW_VERSION [skip ci]"
      ${pkgs.git}/bin/git push origin HEAD:main

      # Create tag for this patch release
      ${pkgs.git}/bin/git tag "v$NEW_VERSION"
      ${pkgs.git}/bin/git push origin "v$NEW_VERSION"

      echo "✅ Successfully bumped and pushed v$NEW_VERSION"
    else
      echo "✅ Version bumped to $NEW_VERSION (local mode - not pushed)"
    fi
  '';

  bump-version = pkgs.writeShellScriptBin "bump-version" ''
    set -euo pipefail

    NEW_VERSION="''${1:-}"

    if [ -z "$NEW_VERSION" ]; then
      echo "Usage: bump-version <version>"
      echo "Example: bump-version 0.2.0"
      exit 1
    fi

    echo "Current version: $(grep '^version =' packages/default/Cargo.toml | cut -d'"' -f2)"
    echo "Setting version to: $NEW_VERSION"

    # Update Cargo.toml
    ${pkgs.gnused}/bin/sed -i "s/^version = \".*\"/version = \"$NEW_VERSION\"/" packages/default/Cargo.toml

    # Commit and push (only if in CI)
    if [ "''${CI:-}" = "true" ]; then
      ${pkgs.git}/bin/git config user.name "Crystal Forge CI"
      ${pkgs.git}/bin/git config user.email "ci@crystal-forge"
      ${pkgs.git}/bin/git add packages/default/Cargo.toml
      ${pkgs.git}/bin/git commit -m "chore: bump to v$NEW_VERSION"
      ${pkgs.git}/bin/git push origin HEAD:main

      ${pkgs.git}/bin/git tag "v$NEW_VERSION"
      ${pkgs.git}/bin/git push origin "v$NEW_VERSION"

      echo "✅ Successfully bumped and pushed v$NEW_VERSION"
    else
      echo "✅ Version bumped to $NEW_VERSION (local mode - not pushed)"
    fi
  '';

  get-version = pkgs.writeShellScriptBin "get-version" ''
    grep '^version =' packages/default/Cargo.toml | cut -d'"' -f2
  '';
in
  bump-patch // {inherit bump-patch bump-version;}
