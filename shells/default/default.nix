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
      openssl.dev
      openssl.out
      fzf
    ];

    shellHook = ''
      echo 🔮 Welcome to the Crystal Forge

      export OPENSSL_DIR=${pkgs.openssl.out}
      export OPENSSL_LIB_DIR=${pkgs.openssl.out}/lib
      export OPENSSL_INCLUDE_DIR=${pkgs.openssl.dev}/include
      export PKG_CONFIG_PATH=${pkgs.openssl.dev}/lib/pkgconfig

      export FZF_DEFAULT_OPTS="--height 40% --reverse --border"

      if [ -n "$BASH_VERSION" ]; then
        . ${pkgs.fzf}/share/fzf/key-bindings.bash
        . ${pkgs.fzf}/share/fzf/completion.bash
      elif [ -n "$ZSH_VERSION" ]; then
        source ${pkgs.fzf}/share/fzf/key-bindings.zsh
        source ${pkgs.fzf}/share/fzf/completion.zsh
      fi
    '';
  }
