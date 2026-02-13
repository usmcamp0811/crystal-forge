{
  lib,
  pkgs,
  ...
}: let
  pname = "crystal-forge-web-ui";
  web-app = pkgs.rustPlatform.buildRustPackage {
    inherit pname;
    version = "0.1.0";
    src = ./.;
    cargoLock.lockFile = ./Cargo.lock;

    nativeBuildInputs = [
      pkgs.rustc
      pkgs.cargo
      pkgs.binaryen
      pkgs.lld
      pkgs.openssl
      pkgs.pkg-config
      pkgs.dioxus-cli
      pkgs.wasm-bindgen-cli
    ];

    buildInputs = [pkgs.openssl.dev pkgs.zlib];
    buildPhase = ''
      export XDG_DATA_HOME=$PWD
      mkdir -p $XDG_DATA_HOME/dioxus/wasm-bindgen
      ln -s ${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen $XDG_DATA_HOME/dioxus/wasm-bindgen/wasm-bindgen-0.2.100

      dx bundle --platform web --release
    '';

    installPhase = ''
      mkdir -p $out/public
      cp -r target/dx/*/release/web/public/* $out/public/

      mkdir -p $out/bin
      cat > $out/bin/${pname} <<EOF
      #!${pkgs.bash}/bin/bash
      PORT=8080
      while [[ \$# -gt 0 ]]; do
        case "\$1" in
          -p|--port)
            PORT="\$2"
            shift 2
            ;;
          *)
            shift
            ;;
        esac
      done
      echo "Running test server on Port: \$PORT" >&2
      exec ${pkgs.python3}/bin/python3 -m http.server "\$PORT" --directory "$out/public"
      EOF
      chmod +x $out/bin/${pname}
    '';
  };
  desktop-app = pkgs.rustPlatform.buildRustPackage {
    inherit pname;
    version = "0.1.0";
    src = ./.;
    cargoLock.lockFile = ./Cargo.lock;

    nativeBuildInputs = [
      pkgs.lld
      pkgs.openssl
      pkgs.pkg-config
      pkgs.dioxus-cli
      pkgs.wasm-bindgen-cli
    ];

    buildInputs = [pkgs.openssl.dev pkgs.zlib];
    buildPhase = ''
      export XDG_DATA_HOME=$PWD
      mkdir -p $XDG_DATA_HOME/dioxus/wasm-bindgen
      ln -s ${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen $XDG_DATA_HOME/dioxus/wasm-bindgen/wasm-bindgen-0.2.100

      dx bundle --platform desktop --release
    '';

    installPhase = ''
      mkdir -p $out
      cp -r target/dx/* $out/

    '';
  };
in
  web-app // {inherit desktop-app;}
