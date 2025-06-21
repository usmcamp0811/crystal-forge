{
  mkShell,
  inputs,
  system,
  pkgs,
  lib,
  ...
}:
with lib;
with lib.crystal-forge;
  mkShell {
    buildInputs = with pkgs; [
      rustc
      cargo
      pkg-config
      openssl
      fzf
      sqlx-cli
    ];

    shellHook = ''
      echo 🔮 Welcome to the Crystal Forge

      export OPENSSL_DIR=${pkgs.openssl.out}
      export OPENSSL_LIB_DIR=${pkgs.openssl.out}/lib
      export OPENSSL_INCLUDE_DIR=${pkgs.openssl.dev}/include
      export PKG_CONFIG_PATH=${pkgs.openssl.dev}/lib/pkgconfig

      export FZF_DEFAULT_OPTS="--height 40% --reverse --border"
      export RUST_LOG=debug
      export CRYSTAL_FORGE__DATABASE__HOST=localhost
      export CRYSTAL_FORGE__DATABASE__PASSWORD=password
      export CRYSTAL_FORGE__DATABASE__USER=crystal_forge
      export DATABASE_URL=postgres://crystal_forge:password@127.0.0.1/crystal_forge
      export CRYSTAL_FORGE__FLAKES__WATCHED__dotfiles=https://gitlab.com/usmcamp0811/dotfiles
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__chesty=Asu0Fl8SsM9Pd/woHt5qkvBdCbye6j2Q2M/qDmnFUjc=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__daly=JhjP4LK72nuTQJ6y7pcYjoTtfrY86BpJBi9WeolcpKY=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__ermy=z9FINYnz2IPPaECHZbTae5prPFUE/ubAT+4HHLPSq7I=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__gray=hUwxCZUFydwDjf8BMyXLyMiI33PrKvhfDRj60OkisdY=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__lucas=OMxvf/rZmi8PZJOpVxjbPHDaX+BmJqp8FUOoosWJ7qY=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__reckless=SKYgYiwK0vMwK3sJP6R53z0gbtOVSWOmJ33WT4AbCQ8=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__webb=ZJBA2GS03P+Q2mhUAbjfjFILQ57yGChjXmRdL6Xfang=
      export CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
      export CRYSTAL_FORGE__SERVER__PORT=3444

      sqlx-refresh() {
        echo "🔄 Resetting and preparing sqlx..."
        sqlx database reset -y
        cargo sqlx prepare
      }

      sqlx-prepare() {
        echo "🛠  Running cargo sqlx prepare..."
        cargo sqlx prepare
      }

      if [ -n "$BASH_VERSION" ]; then
        . ${pkgs.fzf}/share/fzf/key-bindings.bash
        . ${pkgs.fzf}/share/fzf/completion.bash
      elif [ -n "$ZSH_VERSION" ]; then
        source ${pkgs.fzf}/share/fzf/key-bindings.zsh
        source ${pkgs.fzf}/share/fzf/completion.zsh
      fi
    '';
  }
