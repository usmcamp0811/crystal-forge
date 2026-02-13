{ lib, pkgs, ... }:
let
  pname = "crystal-forge-web-ui";
  appName = "crystal-forge-ui";
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
      pkgs.tailwindcss
    ];

    buildInputs = [ pkgs.openssl.dev pkgs.zlib ];
    buildPhase = ''
      export XDG_DATA_HOME=$PWD
      mkdir -p $XDG_DATA_HOME/dioxus/wasm-bindgen
      ln -s ${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen $XDG_DATA_HOME/dioxus/wasm-bindgen/wasm-bindgen-0.2.100

      dx bundle --platform web --release
      ${pkgs.tailwindcss}/bin/tailwindcss -i ${
        ./tailwind.css
      } -o ./assets/tailwind.min.css --minify
    '';

    installPhase = ''
      mkdir -p $out/public
      cp -r target/dx/${appName}/release/web/public/* $out/public/
      cp ./assets/tailwind.min.css $out/public/tailwind.min.css
      cp ./assets/tailwind.min.css $out/public/assets/tailwind.min.css

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
      DOC_ROOT="$out/public" exec ${pkgs.python3}/bin/python3 -c 'import argparse
      import http.server
      import os
      import socketserver

      parser = argparse.ArgumentParser()
      parser.add_argument("--port", type=int, default=int(os.environ.get("PORT", 8080)))
      parser.add_argument("--directory", default=os.environ.get("DOC_ROOT", "."))
      args = parser.parse_args()

      class SpaHandler(http.server.SimpleHTTPRequestHandler):
          def __init__(self, *handler_args, **handler_kwargs):
              super().__init__(*handler_args, directory=args.directory, **handler_kwargs)

          def do_GET(self):
              if not self._has_asset(self.path):
                  self.path = "/index.html"
              return super().do_GET()

          def do_HEAD(self):
              if not self._has_asset(self.path):
                  self.path = "/index.html"
              return super().do_HEAD()

          def _has_asset(self, request_path: str) -> bool:
              path = self.translate_path(request_path)
              return os.path.isfile(path)

      with socketserver.TCPServer(("", args.port), SpaHandler) as httpd:
          httpd.serve_forever()'
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

    buildInputs = [ pkgs.openssl.dev pkgs.zlib ];
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
in web-app // { inherit desktop-app; }
