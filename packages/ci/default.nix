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

    # Check if either "vX.Y.Z" or "X.Y.Z" already exists
    if ${pkgs.git}/bin/git rev-parse "v$NEW_VERSION" >/dev/null 2>&1 || \
       ${pkgs.git}/bin/git rev-parse "$NEW_VERSION" >/dev/null 2>&1; then
      echo "❌ Tag v$NEW_VERSION or $NEW_VERSION already exists. Aborting."
      exit 1
    fi

    # Update Cargo.toml
    ${pkgs.gnused}/bin/sed -i "s/^version = \".*\"/version = \"$NEW_VERSION\"/" packages/default/Cargo.toml

    # Commit and push (only if in CI)
    if [ "''${CI:-}" = "true" ]; then
      ${pkgs.git}/bin/git config user.name "Crystal Forge CI"
      ${pkgs.git}/bin/git config user.email "ci@crystal-forge"
      ${pkgs.git}/bin/git add packages/default/Cargo.toml
      ${pkgs.git}/bin/git commit -m "chore: auto-bump version to $NEW_VERSION [skip ci]"
      ${pkgs.git}/bin/git push origin HEAD:main

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
  cleanup-patch-tags = pkgs.writeShellScriptBin "cleanup-patch-tags" ''
    set -euo pipefail

    KEEP_COUNT=''${1:-10}  # Default to keeping 10 tags, but allow override

    echo "🏷️  Cleaning up patch tags, keeping latest $KEEP_COUNT..."

    # Get all tags, sort by version (semantic sort), filter to current major.minor
    CURRENT_VERSION=$(grep '^version =' packages/default/Cargo.toml | cut -d'"' -f2)
    MAJOR_MINOR=$(echo $CURRENT_VERSION | cut -d'.' -f1-2)

    echo "Current version: $CURRENT_VERSION (keeping tags for $MAJOR_MINOR.x)"

    # Get patch tags for current major.minor version, sorted by patch number (newest first)
    PATCH_TAGS=$(${pkgs.git}/bin/git tag -l "v$MAJOR_MINOR.*" | \
      ${pkgs.gnugrep}/bin/grep -E "^v$MAJOR_MINOR\.[0-9]+$" | \
      ${pkgs.coreutils}/bin/sort -V -r)

    if [ -z "$PATCH_TAGS" ]; then
      echo "No patch tags found for $MAJOR_MINOR.x"
      exit 0
    fi

    TOTAL_TAGS=$(echo "$PATCH_TAGS" | ${pkgs.coreutils}/bin/wc -l)
    echo "Found $TOTAL_TAGS patch tags for $MAJOR_MINOR.x"

    if [ "$TOTAL_TAGS" -le "$KEEP_COUNT" ]; then
      echo "Only $TOTAL_TAGS tags found, nothing to clean up"
      exit 0
    fi

    # Get tags to delete (everything after the first KEEP_COUNT)
    TAGS_TO_DELETE=$(echo "$PATCH_TAGS" | ${pkgs.coreutils}/bin/tail -n +$((KEEP_COUNT + 1)))
    DELETE_COUNT=$(echo "$TAGS_TO_DELETE" | ${pkgs.coreutils}/bin/wc -l)

    echo "Will delete $DELETE_COUNT old patch tags:"
    echo "$TAGS_TO_DELETE"

    # Ask for confirmation unless in CI
    if [ "''${CI:-}" != "true" ]; then
      echo -n "Delete these tags? (y/N): "
      read -r CONFIRM
      if [ "$CONFIRM" != "y" ] && [ "$CONFIRM" != "Y" ]; then
        echo "Cancelled"
        exit 0
      fi
    fi

    # Delete tags locally and remotely
    echo "$TAGS_TO_DELETE" | while IFS= read -r tag; do
      echo "Deleting $tag..."
      ${pkgs.git}/bin/git tag -d "$tag" || true
      if [ "''${CI:-}" = "true" ]; then
        ${pkgs.git}/bin/git push origin ":refs/tags/$tag" || true
      fi
    done

    echo "✅ Cleanup complete!"
  '';
in
  bump-patch // {inherit bump-patch bump-version cleanup-patch-tags;}
