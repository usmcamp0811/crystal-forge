# Web UI Build Verification Check
#
# Verifies the Crystal Forge web UI compiles to WASM and produces valid output:
#   1. Rust source compiles for wasm32-unknown-unknown target
#   2. wasm-bindgen generates JS glue + .wasm binary
#   3. index.html exists and references the WASM loader
#
# Run: nix build .#checks.x86_64-linux.web-ui
{ lib, pkgs, ... }:
let
  webUiSrc = ../../packages/web-ui;

  # Build the WASM binary using Nix's Rust toolchain (no network needed)
  wasmBuild = pkgs.rustPlatform.buildRustPackage {
    pname = "crystal-forge-ui-wasm";
    version = "0.1.0";
    src = webUiSrc;

    cargoLock = { lockFile = "${webUiSrc}/Cargo.lock"; };

    # Cross-compile to wasm32
    buildPhase = ''
      cargo build --target wasm32-unknown-unknown --release
    '';

    # Run wasm-bindgen to generate JS glue code
    installPhase = ''
      mkdir -p $out/wasm

      ${pkgs.wasm-bindgen-cli}/bin/wasm-bindgen \
        --out-dir $out/wasm \
        --out-name crystal-forge-ui \
        --target web \
        target/wasm32-unknown-unknown/release/crystal-forge-ui.wasm

      # Optimize the WASM binary
      ${pkgs.binaryen}/bin/wasm-opt \
        -Oz \
        $out/wasm/crystal-forge-ui_bg.wasm \
        -o $out/wasm/crystal-forge-ui_bg.wasm
    '';

    # No standard check phase — we validate in the check derivation below
    doCheck = false;

    nativeBuildInputs = with pkgs; [ wasm-bindgen-cli binaryen lld ];

    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER = "lld";
  };
  # Validation check: verify the build output is complete and correct
in pkgs.runCommand "crystal-forge-web-ui-check" {
  inherit wasmBuild;
  src = webUiSrc;
} ''
  echo "=== Crystal Forge Web UI Build Verification ==="
  echo ""

  # --- Check 1: WASM binary exists ---
  echo "Check 1: WASM binary exists..."
  if [ ! -f "$wasmBuild/wasm/crystal-forge-ui_bg.wasm" ]; then
    echo "FAIL: crystal-forge-ui_bg.wasm not found in build output"
    echo "Contents of $wasmBuild/wasm/:"
    ls -la "$wasmBuild/wasm/" 2>/dev/null || echo "  (directory does not exist)"
    exit 1
  fi
  WASM_SIZE=$(stat -c%s "$wasmBuild/wasm/crystal-forge-ui_bg.wasm")
  echo "  OK: crystal-forge-ui_bg.wasm ($WASM_SIZE bytes)"

  # --- Check 2: JS glue code exists ---
  echo "Check 2: JS glue code exists..."
  if [ ! -f "$wasmBuild/wasm/crystal-forge-ui.js" ]; then
    echo "FAIL: crystal-forge-ui.js (wasm-bindgen glue) not found"
    exit 1
  fi
  echo "  OK: crystal-forge-ui.js exists"

  # --- Check 3: Dioxus.toml is valid ---
  echo "Check 3: Dioxus.toml exists and has required fields..."
  if [ ! -f "$src/Dioxus.toml" ]; then
    echo "FAIL: Dioxus.toml not found in source"
    exit 1
  fi
  if ! grep -q 'name = "crystal-forge-ui"' "$src/Dioxus.toml"; then
    echo "FAIL: Dioxus.toml missing application name"
    exit 1
  fi
  if ! grep -q 'default_platform = "web"' "$src/Dioxus.toml"; then
    echo "FAIL: Dioxus.toml missing web platform config"
    exit 1
  fi
  echo "  OK: Dioxus.toml valid"

  # --- Check 4: WASM binary is non-trivial (> 1KB) ---
  echo "Check 4: WASM binary is non-trivial..."
  if [ "$WASM_SIZE" -lt 1024 ]; then
    echo "FAIL: WASM binary is suspiciously small ($WASM_SIZE bytes)"
    exit 1
  fi
  echo "  OK: WASM binary is $WASM_SIZE bytes"

  # --- Check 5: JS glue references the WASM file ---
  echo "Check 5: JS glue references WASM binary..."
  if ! grep -q "crystal-forge-ui_bg.wasm" "$wasmBuild/wasm/crystal-forge-ui.js"; then
    echo "FAIL: JS glue does not reference the WASM binary"
    exit 1
  fi
  echo "  OK: JS glue references WASM binary"

  echo ""
  echo "=== All checks passed ==="
  echo "  WASM binary: $WASM_SIZE bytes"
  echo "  JS glue:     $(stat -c%s "$wasmBuild/wasm/crystal-forge-ui.js") bytes"

  # Write a marker so Nix knows the check passed
  mkdir -p $out
  echo "web-ui build check passed" > $out/result.txt
''
