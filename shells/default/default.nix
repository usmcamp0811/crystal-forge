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

      export OPENSSL_DIR=${pkgs.openssl.dev}
      export PKG_CONFIG_PATH=${pkgs.openssl.dev}/lib/pkgconfig

      export FZF_DEFAULT_OPTS="--height 40% --reverse --border"
      . ${pkgs.fzf}/share/fzf/key-bindings.bash
      . ${pkgs.fzf}/share/fzf/completion.bash
    '';
  }
