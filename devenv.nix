{ pkgs, lib, config, inputs, ... }:

# TASK-462.1, Phase 1: an additive, devenv-native reproduction of
# `run-ui-dev` (PostgreSQL + Crystal Forge API server + Dioxus web UI dev
# server) with per-worktree isolation and dynamically allocated ports, so
# two Git worktrees can run this stack at the same time.
#
# See docs/agents/devenv-workflow.md for how to start/stop this stack, how
# port/hostname isolation is achieved, and how this relates to the existing
# `nix develop` / `run-ui-dev` / process-compose workflow, which this file
# does not replace, remove, or modify.
#
# This is devenv's NATIVE project format (this file plus devenv.yaml),
# driven by the real `devenv` CLI, not devenv's Nix-flake integration
# (`devenv.lib.mkShell`). That distinction is load-bearing, not stylistic:
# `processes.<name>.ports.<port>.value` is only resolved to a genuinely
# free port when devenv's own compiled Nix backend evaluates this module
# (its custom `allocatePort` primop, see `devenv-processes/src/config.rs`
# upstream). Flake integration evaluates the same module through plain
# `nix build`/`nix develop`, which never has that primop, so
# `ports.<port>.value` silently equals the requested base port, unchanged,
# even when that port is already bound elsewhere — verified empirically
# while implementing this task (see TASK-462.1's verification notes: an
# occupied base port still round-tripped as the "resolved" value under
# flake integration, while the native CLI correctly skipped to the next
# free port). `flake.nix`'s additive `devShells.devenv` output exists only
# to put the real `devenv` CLI on `PATH`; entering it and running
# `devenv up` from this worktree's root is what actually evaluates this
# file.
let
  system = pkgs.stdenv.hostPlatform.system;

  dioxusCli = inputs.nixpkgsDioxusCli.legacyPackages.${system}.dioxus-cli;

  # Development-only password for the PostgreSQL role this module creates.
  # Matches the fixed dev password `run-ui-dev`/`db-only` already use
  # (packages/devScripts/default.nix's `db_password`); not a production
  # secret, and the socket this instance listens on (see `services.postgres`
  # below) is not reachable outside this worktree's own filesystem.
  dbPassword = "password";

  # PostgreSQL isolation mechanism (TASK-462.1 AC #4): a dynamically
  # allocated per-worktree TCP port, using devenv's automatic port
  # allocation (`processes.postgres.ports.main.allocate`, wired
  # automatically by `services.postgres` whenever `listen_addresses` is
  # non-empty — see the upstream `services/postgres.nix`). Two properties
  # fall out of that for free, without any bespoke Nix code: the resolved
  # port lives at `config.processes.postgres.ports.main.value` (devenv's
  # own primop finds a free port starting from `port` below, exactly like
  # the API/web ports); and the on-disk data directory lives under
  # `config.env.DEVENV_STATE` (`${config.devenv.root}/.devenv/state`), so
  # two worktrees never share a data directory even before considering
  # ports at all.
  #
  # A per-worktree Unix socket (`listen_addresses = "";`, this module's
  # default) was tried first, since it also sidesteps Portless entirely
  # (no port means no HTTP route to generate) and does not need a port at
  # all. It does not integrate cleanly with the actual Crystal Forge
  # server, though: `crates/cf-server/src/config/database.rs` builds its
  # connection string as a plain
  # `postgres://{user}:{password}@{host}:{port}/{name}` URL, which cannot
  # represent a Unix socket directory as `host` (verified empirically: the
  # server failed every connection attempt with `both host and hostaddr
  # are missing`, libpq's error for a host segment it cannot parse from
  # that URL shape). Changing that parsing is an application-behavior
  # change this task does not make, so this module uses TCP instead.
  #
  # Portless would still generate an inert `.localhost` HTTP route for
  # `postgres` if enabled, since `services.postgres` does not expose a
  # way to opt a process out of Portless route generation. PostgreSQL does
  # not speak HTTP, so that route can never actually carry PostgreSQL
  # traffic; ignore it if Portless is enabled (see
  # docs/agents/devenv-workflow.md).
  dbHost = "127.0.0.1";
  dbPort = config.processes.postgres.ports.main.value;

  # API server and web UI dev server ports: devenv's automatic port
  # allocation (TASK-462.1 AC #3), preferring the same bases run-ui-dev
  # uses today so the common case (only one worktree's stack running)
  # resolves to the same familiar 3445/8080, while a second concurrent
  # worktree transparently gets the next free port instead of colliding.
  apiPort = config.processes.api.ports.http.value;
  webPort = config.processes.web.ports.http.value;

  fixturePath =
    "${config.devenv.root}/docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json";
  webUiDir = "${config.devenv.root}/packages/web-ui";

  tomlFormat = pkgs.formats.toml { };

  # Mirrors packages/devScripts/default.nix's `configTemplateMock` (mock
  # fixture data, `AUTH_MODE=local` bootstrap admin, mock execution mode),
  # but with this worktree's resolved database/API ports baked in at
  # Nix-evaluation time instead of the fixed 3042/3445 `run-ui-dev` uses,
  # and a Unix-socket `database.host` instead of a TCP address. This is a
  # deliberately separate template: two concurrent worktrees have different
  # resolved ports and socket directories, so they can never safely share
  # `run-ui-dev`'s generator or its output file (see
  # `packages/devScripts/default.nix`'s `generateConfig`/`envExports`,
  # which this module does not call or modify).
  configTemplateMock = tomlFormat.generate "crystal-forge-config-devenv-mock.toml" {
    database = {
      host = dbHost;
      port = dbPort;
      user = "crystal_forge";
      password = dbPassword;
      name = "crystal_forge";
    };
    server = {
      host = "127.0.0.1";
      port = apiPort;
    };
    build = {
      cores = 7;
      max_jobs = 1;
      poll_interval = "1m";
    };
    builder = {
      enable_api_mode = true;
      builder_id = "00000000-0000-0000-0000-000000000001";
      # Substituted at process-start time (see `generateDevenvConfig`
      # below) once the per-worktree key directory is known to exist.
      private_key_path = "BUILDER_KEY_PATH_PLACEHOLDER";
      server_url = "http://127.0.0.1:${toString apiPort}";
      poll_interval = 5;
      heartbeat_interval = 30;
      max_concurrent_jobs = 1;
    };
    client = {
      server_host = "127.0.0.1";
      server_port = apiPort;
      # A literal, unexpanded shell variable reference, exactly like
      # `packages/devScripts/default.nix`'s templates: the Crystal Forge
      # config loader expands `$CF_KEY_DIR` from the process environment
      # when it reads this field, so the value only needs to be correct
      # relative to whatever `CF_KEY_DIR` that environment sets (see
      # `generateDevenvConfig`, which sets the same `CF_KEY_DIR` the API
      # server process below also exports before reading this file).
      private_key = "$CF_KEY_DIR/agent.key";
    };
    # Pre-populated mock data, matching run-ui-dev's fixture-seeded demo
    # environment/system so the UI has something to render immediately.
    environments = [{
      name = "mockenv";
      description =
        "An environment full of agents created from shell scripts for testing purposes";
      is_active = true;
      risk_profile = "LOW";
      compliance_level = "NONE";
    }];
    systems = [{
      hostname = "test.gray";
      public_key = pkgs.crystal-forge.testAgents.test-gray.publicKey;
      environment = "mockenv";
      flake_name = "dotfiles";
    }];
    flakes = {
      flake_polling_interval = "10m";
      commit_evaluation_interval = "10m";
      build_processing_interval = "10m";
      watched = [{
        name = "dotfiles";
        repo_url = "https://gitlab.com/usmcamp0811/dotfiles";
        auto_poll = false;
        initial_commit_depth = 10;
      }];
    };
  };

  # Generates this worktree's config file under devenv's own per-worktree
  # state directory (`$DEVENV_STATE`, itself under `.devenv/state` inside
  # `config.devenv.root`), never inside the repository source tree and
  # never at `run-ui-dev`'s shared `$XDG_RUNTIME_DIR/crystal-forge/`
  # location. Development agent/builder/cache keys stay at the same
  # machine-wide `$CF_KEY_DIR` every other dev workflow in this repository
  # already uses (see `packages/devScripts/default.nix`'s `envExports`):
  # that identity is intentionally shared across worktrees on one machine,
  # not per-worktree, and generating it here if missing preserves that.
  generateDevenvConfig = pkgs.writeShellApplication {
    name = "generate-devenv-config";
    runtimeInputs = [ pkgs.coreutils pkgs.crystal-forge.default.cf-keygen-drv ];
    text = ''
      set -euo pipefail
      CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      mkdir -p "$CF_KEY_DIR"
      if [[ ! -f "$CF_KEY_DIR/agent.key" ]]; then
        echo "Generating agent keys..."
        cf-keygen -f "$CF_KEY_DIR/agent.key"
      fi
      if [[ ! -f "$CF_KEY_DIR/builder.key" ]]; then
        echo "Generating builder keys..."
        cf-keygen -f "$CF_KEY_DIR/builder.key"
      fi
      if [[ ! -f "$CF_KEY_DIR/cache-encryption.key" ]]; then
        echo "Generating cache encryption key..."
        head -c 48 /dev/urandom | base64 > "$CF_KEY_DIR/cache-encryption.key"
        chmod 600 "$CF_KEY_DIR/cache-encryption.key"
      fi
      CONFIG_FILE="${config.env.DEVENV_STATE}/crystal-forge-config.toml"
      sed -e "s|BUILDER_KEY_PATH_PLACEHOLDER|$CF_KEY_DIR/builder.key|g" \
        ${configTemplateMock} > "$CONFIG_FILE"
      echo "$CONFIG_FILE"
    '';
  };
in
{
  # Reuse this repository's own Snowfall-generated package namespace
  # (`pkgs.crystal-forge.*`) instead of re-deriving cf-server, cf-keygen,
  # and the test-agent fixtures a second time. `crystalForge` is this
  # flake itself (devenv.yaml: `crystalForge.url: path:.`); its
  # `overlays.default` is the same overlay Snowfall applies internally
  # when building `packages/devScripts/default.nix` and every other
  # package in this repository.
  overlays = [ inputs.crystalForge.overlays.default ];

  # Worktree identity for Portless `.localhost` hostnames (TASK-462.1
  # AC #5): deliberately left unset.
  #
  # devenv's own hostname resolution (`project_name` in the upstream
  # `devenv` CLI's `proxy.rs`) already falls back to the basename of
  # `devenv.root` — this worktree's absolute directory, following
  # docs/agents/worktrees.md's `TASK-ID-short-slug` convention — whenever
  # `name` is left at its module default (`"devenv-shell"`). That gives
  # every worktree a distinct, stable identity with zero bespoke Nix code:
  # same worktree, same identity, across restarts; different worktrees,
  # different identities, automatically sanitized to a valid `.localhost`
  # label (lowercased, non-alphanumeric runs collapsed to a single `-`) by
  # that same function. Setting `name` here to a fixed literal would
  # instead make every worktree collide on the same Portless hostnames,
  # exactly what this task must avoid. See TASK-462.1's verification notes
  # for the two-worktree proof of this fallback.

  # `web-ui-test` (TASK-462.1 AC #7): the exact same command
  # `shells/default/default.nix`'s legacy shell aliases to
  # `nix run .#devScripts.webUiTest`, put directly on `PATH` here instead
  # of a `nix run` alias, unmodified. It already reads `CF_UI_DEV_BASE_URL`
  # / `CF_UI_DEV_API_BASE_URL` / `DB_HOST` / `DB_PORT` / `DB_USER` /
  # `DB_PASSWORD` / `DB_NAME` as overrides (see
  # checks/web-ui/tests/web-ui-test.sh), all exported above via `env`.
  # `wasm-bindgen-cli_0_2_108`/`binaryen`/`lld` must be on `PATH` for `dx
  # serve` to find the pinned wasm-bindgen version instead of whatever
  # version its own dependency closure would otherwise resolve first,
  # exactly matching run-ui-dev/run-ui-frontend's `runtimeInputs` (see
  # packages/devScripts/default.nix).
  packages = [
    pkgs.postgresql
    pkgs.curl
    pkgs.crystal-forge.devScripts.webUiTest
    pkgs.wasm-bindgen-cli_0_2_108
    pkgs.binaryen
    pkgs.lld
  ];

  # Portless stays opt-in (TASK-462.1 AC #6). On Linux, enabling it makes
  # devenv ask for sudo authentication to bind the shared proxy's port-80
  # listener; nothing in this stack may depend on that succeeding. Flip
  # this locally (`process.proxy.enable = true;` in a private override, or
  # `devenv.local.nix` — see docs/agents/devenv-workflow.md) to try it.
  process.proxy.enable = lib.mkDefault false;

  services.postgres = {
    enable = true;
    # TCP, loopback-only: see the `dbHost`/`dbPort` comment above for why
    # this module uses a dynamically allocated TCP port rather than a Unix
    # socket. `port` is only the requested *base*; devenv's automatic
    # allocation resolves the actual bound port (`dbPort` above).
    listen_addresses = "127.0.0.1";
    port = 5432;
    initialDatabases = [{
      name = "crystal_forge";
      user = "crystal_forge";
      pass = dbPassword;
    }];
  };

  processes.api = {
    exec = ''
      set -euo pipefail
      export CF_KEY_DIR="''${XDG_DATA_HOME:-$HOME/.local/share}/crystal-forge/devkeys"
      CONFIG_FILE="$(${generateDevenvConfig}/bin/generate-devenv-config)"
      export CRYSTAL_FORGE_CONFIG="$CONFIG_FILE"
      export CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY="$(cat "$CF_KEY_DIR/cache-encryption.key")"
      export FIXTURE_JSON_PATH="${fixturePath}"
      export AUTH_MODE=local
      export CRYSTAL_FORGE_LOCAL_BOOTSTRAP_USERNAME=admin
      export CRYSTAL_FORGE_LOCAL_BOOTSTRAP_PASSWORD=password
      export CRYSTAL_FORGE_LOCAL_BOOTSTRAP_EMAIL=admin@crystal-forge.local
      # Mock execution mode, matching run-ui-dev: this dev loop exercises
      # the UI and API, not real Nix evaluation/build/scan execution.
      export CRYSTAL_FORGE__SERVER__EXECUTION_MODE=mock
      export RUST_LOG="info,crystal_forge::fixtures::seed=debug"
      # The core backend (no embedded UI build): the Dioxus dev server
      # below provides the UI, so building an unused embedded copy of it
      # into the API server on every UI source change would be wasted
      # work (same rationale as run-ui-dev; see
      # packages/devScripts/default.nix's `runUiDev`).
      exec ${pkgs.crystal-forge.default.cf-server-core-drv}/bin/server
    '';
    ports.http.allocate = 3445;
    after = [ "devenv:processes:postgres" ];
    ready.http.get = {
      port = apiPort;
      path = "/status";
    };
  };

  processes.web = {
    exec = ''
      set -euo pipefail
      expected_dx_version="0.7.3"
      actual_dx_version="$(${dioxusCli}/bin/dx --version | ${pkgs.gawk}/bin/awk 'NR == 1 { print $2 }')"
      if [[ "$actual_dx_version" != "$expected_dx_version" ]]; then
        echo "Incompatible Dioxus CLI version; expected $expected_dx_version, got $actual_dx_version." >&2
        exit 1
      fi

      # Pin the exact wasm-bindgen dx looks for, the same way
      # run-ui-dev/run-ui-frontend do (see
      # packages/devScripts/default.nix's `runUiDev`/`runUiFrontend`).
      export XDG_DATA_HOME="''${XDG_DATA_HOME:-$HOME/.local/share}"
      mkdir -p "$XDG_DATA_HOME/dioxus/wasm-bindgen"
      ln -sf "${pkgs.wasm-bindgen-cli_0_2_108}/bin/wasm-bindgen" \
        "$XDG_DATA_HOME/dioxus/wasm-bindgen/wasm-bindgen-0.2.108"

      ${pkgs.tailwindcss_4}/bin/tailwindcss \
        -i "${webUiDir}/tailwind.css" \
        -o "${webUiDir}/assets/tailwind.min.css" \
        --minify

      # Baked into this build at compile time (see
      # packages/web-ui/src/api/client.rs's `backend_origin_for_dev`): the
      # Dioxus dev server's own default backend-detection heuristic
      # assumes a fixed API port (3445), which is exactly the fixed-port
      # assumption this task replaces. `CF_UI_DEV_API_PORT` is a no-op
      # everywhere else; only this devenv workflow ever sets it, so
      # run-ui-dev's identical-looking dev server keeps its existing
      # behavior unchanged.
      export CF_UI_DEV_API_PORT="${toString apiPort}"

      cd "${webUiDir}"
      exec ${dioxusCli}/bin/dx serve --platform web \
        --addr 127.0.0.1 --port ${toString webPort} \
        --open false --interactive false
    '';
    ports.http.allocate = 8080;
    after = [ "devenv:processes:api" ];
  };

  # Exported through `devenv shell`/`devenv up` into this worktree's own
  # environment (TASK-462.1 AC #7 and the web-ui-test.sh preconditions it
  # reads, see checks/web-ui/tests/web-ui-test.sh): `CF_UI_DEV_BASE_URL`
  # and `CF_UI_DEV_API_BASE_URL` are the same variables that script already
  # accepts as overrides, and `DB_HOST`/`DB_PORT`/`DB_USER`/`DB_PASSWORD`/
  # `DB_NAME` are what it falls back to for its fixture-backed workflows'
  # direct PostgreSQL precondition check. Neither web-ui-test.sh nor its
  # underlying integration-test.js needs to change to consume these; they
  # already read exactly this set of variables.
  env = {
    DB_HOST = dbHost;
    DB_PORT = toString dbPort;
    DB_USER = "crystal_forge";
    DB_PASSWORD = dbPassword;
    DB_NAME = "crystal_forge";
    CF_UI_DEV_BASE_URL = "http://127.0.0.1:${toString webPort}";
    CF_UI_DEV_API_BASE_URL = "http://127.0.0.1:${toString apiPort}";
  };

  enterShell = ''
    echo "🔮 Crystal Forge devenv workflow (TASK-462.1)"
    echo ""
    echo "  Worktree root:  ${config.devenv.root}"
    echo "  PostgreSQL:     127.0.0.1:${toString dbPort} (crystal_forge/crystal_forge)"
    echo "  API server:     http://127.0.0.1:${toString apiPort}"
    echo "  Web UI:         http://127.0.0.1:${toString webPort}"
    echo ""
    echo "  devenv up          → start PostgreSQL + API server + web UI dev server"
    echo "  devenv processes list  → show resolved ports and process status"
    echo "  devenv down        → stop only this worktree's stack"
    echo "  web-ui-test        → run against the URLs above (needs: nix develop)"
    echo ""
    echo "  Portless (.localhost URLs): disabled by default; see"
    echo "  docs/agents/devenv-workflow.md to enable it."
    echo ""
  '';
}
