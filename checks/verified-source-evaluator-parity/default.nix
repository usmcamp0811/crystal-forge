{ pkgs, ... }:

# Exercises the production store-source contract from one canonical Git archive.
# Ambient environment and filesystem state must not change pure drvPath.
let
  primaryExpression =
    ../../packages/default/crates/cf-server/src/models/primary_evaluation.nix;
  authoritativeExpression = ''
    (${builtins.readFile primaryExpression}) {
      flakeRef = "__SOURCE_REF__";
      requestedRevision = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
      policyCheckers.host = _: {};
    }
  '';
  canonicalSource = pkgs.runCommand "crystal-forge-canonical-source" {
    nativeBuildInputs = [ pkgs.git pkgs.gnutar ];
  } ''
    cp -R ${./fixture} repository
    chmod -R u+w repository
    chmod +x repository/bin/run
    ln -s bin/run repository/run
    git -C repository init -q
    git -C repository -c user.name=fixture -c user.email=fixture@example.invalid add .
    git -C repository -c user.name=fixture -c user.email=fixture@example.invalid commit -qm fixture
    commit="$(git -C repository rev-parse HEAD)"
    git -C repository archive --format=tar --output="$TMPDIR/source.tar" "$commit"

    mkdir "$out"
    tar -xf "$TMPDIR/source.tar" -C "$out"
    test -x "$out/bin/run"
    test "$(readlink "$out/run")" = bin/run
    test "$(cat "$out/revision.txt")" = "$commit"
  '';
in
pkgs.runCommand "crystal-forge-verified-source-evaluator-parity" {
  nativeBuildInputs = [ pkgs.jq pkgs.nix pkgs.nix-eval-jobs ];
} ''
  export HOME="$TMPDIR/home"
  export XDG_CACHE_HOME="$TMPDIR/cache"
  export NIX_CONFIG='experimental-features = nix-command flakes
  pure-eval = false'
  mkdir -p "$HOME"
  source_a=${canonicalSource}
  source_b=${canonicalSource}
  test "$source_a" = "$source_b"
  test "$(nix hash path --type sha256 --sri "$source_a")" = \
    "$(nix hash path --type sha256 --sri "$source_b")"
  source_nar_hash="$(nix hash path --type sha256 --sri "$source_a")"
  encoded_nar_hash="$(jq -rn --arg value "$source_nar_hash" '$value | @uri')"
  source_ref="path:$source_a?narHash=$encoded_nar_hash"

  export CF_VERIFIED_SOURCE_HOST_VALUE=host-specific
  touch /tmp/crystal-forge-verified-source-host-path
  expression='${authoritativeExpression}'
  expression="''${expression/__SOURCE_REF__/$source_ref}"
  nix-eval-jobs \
    --expr "$expression" \
    --meta \
    --apply 'derivation: derivation.meta.policies' \
    --option pure-eval true \
    --option allow-import-from-derivation true \
    --workers 1 \
    --max-memory-size 0 > server-result.jsonl
  server_drv="$(jq -er 'select(.attrPath == ["host"] and .error == null) | .drvPath' \
    server-result.jsonl)"
  builder_drv="$(nix eval --raw --no-write-lock-file \
    --option pure-eval true \
    --option allow-import-from-derivation true \
    "$source_ref#nixosConfigurations.host.config.system.build.toplevel.drvPath")"
  test "$server_drv" = "$builder_drv"
  case "$server_drv" in *verified-source-pure.drv) ;; *) exit 1 ;; esac

  impure_drv="$(nix eval --raw --impure --no-write-lock-file \
    --option allow-import-from-derivation true \
    "$source_ref#nixosConfigurations.host.config.system.build.toplevel.drvPath")"
  test "$impure_drv" != "$server_drv"
  touch "$out"
''
