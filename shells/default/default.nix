{
  mkShell,
  system,
  inputs,
  pkgs,
  lib,
  ...
}:
with lib;
with lib.crystal-forge; let
  db_port = 3042;
  db_password = "password";
  cf_port = 3445;

  keyPair = pkgs.runCommand "agent-keypair" {} ''
    mkdir -p $out
    ${pkgs.crystal-forge.agent.cf-keygen}/bin/cf-keygen -f $out/agent.key
  '';
  key = pkgs.runCommand "agent.key" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.key $out/
  '';
  pub = pkgs.runCommand "agent.pub" {} ''
    mkdir -p $out
    cp ${keyPair}/agent.pub $out/
  '';
  myServicesMod = pkgs.process-compose-flake.evalModules {
    modules = [
      inputs.services-flake.processComposeModules.default
      {
        settings.processes.server = {
          command = pkgs.writeShellApplication {
            runtimeInputs = [pkgs.nix pkgs.crystal-forge.server];
            text = ''
              export RUST_LOG=debug
              export CRYSTAL_FORGE__DATABASE__HOST=127.0.0.1
              export CRYSTAL_FORGE__DATABASE__PORT=${toString db_port}
              export CRYSTAL_FORGE__DATABASE__USER=crystal_forge
              export CRYSTAL_FORGE__DATABASE__PASSWORD=${toString db_password}
              export CRYSTAL_FORGE__DATABASE__NAME=crystal_forge
              export DATABASE_URL=postgres://crystal_forge:${db_password}@127.0.0.1:${toString db_port}/crystal_forge
              export CRYSTAL_FORGE__FLAKES__WATCHED__dotfiles=https://gitlab.com/usmcamp0811/dotfiles
              export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__dev=
              export CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
              export CRYSTAL_FORGE__SERVER__PORT=${toString cf_port}

              ${pkgs.crystal-forge.server}/bin/server
            '';
            name = "crystal-forge-server";
          };
          depends_on."crystal-forge-db".condition = "process_healthy";
          readiness_probe = {
            exec.command = "${pkgs.postgresql}/bin/pg_isready -h 127.0.0.1 -p ${toString db_port} -U crystal_forge -d crystal_forge";
            initial_delay_seconds = 2;
            period_seconds = 5;
            timeout_seconds = 3;
            success_threshold = 1;
            failure_threshold = 5;
          };
        };
        services.postgres."crystal-forge-db" = {
          enable = true;
          listen_addresses = "0.0.0.0";

          port = db_port;
          initialScript.before = ''
            CREATE USER crystal_forge LOGIN;
            CREATE DATABASE crystal_forge OWNER crystal_forge;
            GRANT ALL PRIVILEGES ON DATABASE crystal_forge TO crystal_forge;
          '';
          initialDatabases = [
          ];
        };
      }
    ];
  };
in
  mkShell {
    buildInputs = with pkgs; [
      rustc
      cargo
      pkg-config
      openssl
      fzf
      postgresql
      sqlx-cli
      myServicesMod.config.outputs.package
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
      export CRYSTAL_FORGE__DATABASE__PORT=${toString db_port}
      export CRYSTAL_FORGE__DATABASE__USER=crystal_forge
      export CRYSTAL_FORGE__DATABASE__PASSWORD=${db_password}
      export CRYSTAL_FORGE__DATABASE__NAME=crystal_forge
      export DATABASE_URL=postgres://crystal_forge:${db_password}@127.0.0.1:${toString db_port}/crystal_forge
      export CRYSTAL_FORGE__FLAKES__WATCHED__dotfiles=https://gitlab.com/usmcamp0811/dotfiles
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__chesty=Asu0Fl8SsM9Pd/woHt5qkvBdCbye6j2Q2M/qDmnFUjc=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__daly=JhjP4LK72nuTQJ6y7pcYjoTtfrY86BpJBi9WeolcpKY=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__ermy=z9FINYnz2IPPaECHZbTae5prPFUE/ubAT+4HHLPSq7I=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__gray=hUwxCZUFydwDjf8BMyXLyMiI33PrKvhfDRj60OkisdY=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__lucas=OMxvf/rZmi8PZJOpVxjbPHDaX+BmJqp8FUOoosWJ7qY=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__reckless=SKYgYiwK0vMwK3sJP6R53z0gbtOVSWOmJ33WT4AbCQ8=
      export CRYSTAL_FORGE__SERVER__AUTHORIZED_KEYS__webb=ZJBA2GS03P+Q2mhUAbjfjFILQ57yGChjXmRdL6Xfang=
      export CRYSTAL_FORGE__SERVER__HOST=0.0.0.0
      export CRYSTAL_FORGE__SERVER__PORT=${toString cf_port}

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
