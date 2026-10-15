{ pkgs, lib, ... }:
with lib;
with lib.crystal-forge;
let
  # Script to run complexity analysis and generate reports
  complexity-report = pkgs.writeShellApplication {
    name = "complexity-report";
    runtimeInputs = with pkgs; [
      cargo
      clippy
      rustc
      coreutils
      findutils
      gcc
      gnugrep
      jq
      pkg-config
      tokei
    ];
    # Disable shellcheck for style warnings that are intentional
    excludeShellChecks = [ "SC2129" "SC2016" ];
    text = ''
          set -euo pipefail
          umask 077

          PROJECT_ROOT="''${PROJECT_ROOT:-$(pwd)}"
          OUTPUT_DIR="''${OUTPUT_DIR:-$PROJECT_ROOT/complexity-report}"

          mkdir -p "$OUTPUT_DIR"

          # Match the native OpenSSL inputs used by workspace packaging. A
          # runnable pkg-config alone cannot discover Nix development outputs.
          export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
          export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
          export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
          export SQLX_OFFLINE=true
          # All-features analysis includes embedded-ui. Use real source assets;
          # compiling the asset embedding does not require a WASM bundle build.
          export CRYSTAL_FORGE_UI_DIST="$PROJECT_ROOT/packages/web-ui/assets"
          # Analysis needs no debug information. Two concurrent compiler jobs
          # bound peak resource use on shared CI runners without skipping crates.
          export CARGO_BUILD_JOBS=2
          export CARGO_PROFILE_DEV_DEBUG=0
          export CARGO_PROFILE_TEST_DEBUG=0

          echo "🔍 Analyzing code complexity..."
          echo "   Project root: $PROJECT_ROOT"
          echo "   Output dir: $OUTPUT_DIR"

          # Retain application tests in the existing source scope; exclude
          # build outputs, gate fixtures, and dependency directories.
          echo "📁 Finding Rust source files..."
          find "$PROJECT_ROOT" -type f -name "*.rs" \
            ! -path "*/target/*" \
            ! -path "*/.git/*" \
            ! -path "*/checks/*" \
            ! -path "*/result/*" \
            ! -path "*/node_modules/*" \
            > "$OUTPUT_DIR/rust-files.txt"

          FILE_COUNT=$(wc -l < "$OUTPUT_DIR/rust-files.txt")
          echo "   Found $FILE_COUNT Rust files"

          # Run tokei for LOC statistics
          echo "📊 Generating LOC statistics..."
          tokei --output json "$PROJECT_ROOT" > "$OUTPUT_DIR/tokei-report.json"

          # Run cargo clippy with complexity lints on all packages
          echo "🔎 Running clippy complexity lints..."

          CLIPPY_OUTPUT="$OUTPUT_DIR/clippy-output.txt"
          : > "$CLIPPY_OUTPUT"
          : > "$OUTPUT_DIR/clippy-messages.jsonl"
          echo '[]' > "$OUTPUT_DIR/clippy-status.json"
          ANALYSIS_FAILED=0

          # List of packages to analyze
          PACKAGES=("packages/default" "packages/web-ui")

          # Export clippy lints via environment variable
          export RUSTFLAGS="-Wclippy::complexity -Wclippy::cognitive_complexity -Wclippy::too_many_arguments -Wclippy::too_many_lines -Wclippy::type_complexity -Wclippy::fn_params_excessive_bools -Wclippy::vec_box"

          for pkg in "''${PACKAGES[@]}"; do
              echo "   START clippy $pkg $(date -u '+%Y-%m-%dT%H:%M:%SZ') jobs=$CARGO_BUILD_JOBS"
              start=$SECONDS
              status=0
              package_log="$OUTPUT_DIR/''${pkg//\//-}"
              # Keep diagnostics as report data, but never turn a failed
              # compiler/build script into a successful complexity analysis.
              (cd "$PROJECT_ROOT/$pkg" && cargo clippy --all-targets --all-features --message-format=json) \
                > "$package_log.jsonl" 2> "$package_log.stderr" || status=$?
              cat "$package_log.jsonl" >> "$OUTPUT_DIR/clippy-messages.jsonl"
              cat "$package_log.stderr" >> "$CLIPPY_OUTPUT"
              jq -r 'select(.reason == "compiler-message") | .message.rendered // .message.message' \
                "$package_log.jsonl" >> "$CLIPPY_OUTPUT"
              # Print only bounded real errors; complete diagnostics are artifacts.
              jq -rs '[.[] | select(.reason == "compiler-message" and .message.level == "error") |
                "\(.message.spans[0].file_name // "unknown"):\(.message.spans[0].line_start // 0): \(.message.code.code // "compiler"): \(.message.message)"] | .[-10:][]' "$package_log.jsonl"
              tail -n 10 "$package_log.stderr"
              echo "   END clippy $pkg exit=$status elapsed=$((SECONDS - start))s $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
              jq --arg package "$pkg" --argjson status "$status" \
                --argjson elapsed "$((SECONDS - start))" \
                '. + [{package: $package, exit_status: $status, elapsed_seconds: $elapsed}]' \
                "$OUTPUT_DIR/clippy-status.json" > "$OUTPUT_DIR/clippy-status.json.tmp"
              mv "$OUTPUT_DIR/clippy-status.json.tmp" "$OUTPUT_DIR/clippy-status.json"
              if [[ $status -ne 0 ]]; then
                ANALYSIS_FAILED=1
              fi
          done

          cd "$PROJECT_ROOT"

          # Parse clippy output for violations
          echo "📋 Parsing complexity violations..."

          # Match diagnostic codes, not prose or help links. All-targets can
          # repeat a diagnostic, so count each source diagnostic only once.
          jq -s '[.[] | select(.reason == "compiler-message") | .message |
            select(.code.code? | . != null and test("^clippy::(cognitive_complexity|too_many_arguments|too_many_lines|type_complexity|fn_params_excessive_bools|vec_box)$"))] |
            unique_by([.code.code, .message, .spans])' \
            "$OUTPUT_DIR/clippy-messages.jsonl" > "$OUTPUT_DIR/complexity-violations.json"
          jq -r '.[] | (.spans[0] // {}) as $s |
            "\($s.file_name // "unknown"):\($s.line_start // 0): \(.code.code): \(.message)"' \
            "$OUTPUT_DIR/complexity-violations.json" > "$OUTPUT_DIR/complexity-violations.txt"

          VIOLATION_COUNT=$(wc -l < "$OUTPUT_DIR/complexity-violations.txt" 2>/dev/null || echo "0")
          echo "   Found $VIOLATION_COUNT complexity violations"

          # Generate per-file metrics
          echo "📈 Computing per-file metrics..."
          
          # Initialize JSON file properly
          echo '{"files":[]}' > "$OUTPUT_DIR/file-metrics.json"
          
          # Create a temp file for building the files array
          TEMP_FILES=$(mktemp)
          echo '[' > "$TEMP_FILES"
          
          FIRST=true
          while IFS= read -r file; do
            if [[ -f "$file" ]]; then
              rel_path="''${file#"$PROJECT_ROOT/"}"
              # Ensure we get numeric values, defaulting to 0
              total_lines=$(wc -l < "$file" | tr -d '[:space:]') || total_lines=0
              fn_count=$(grep -c "^\\s*\\(pub\\s\\+\\)\\?\\(async\\s\\+\\)\\?fn\\s" "$file" 2>/dev/null | tr -d '[:space:]') || fn_count=0
              struct_count=$(grep -c "^\\s*\\(pub\\s\\+\\)\\?struct\\s" "$file" 2>/dev/null | tr -d '[:space:]') || struct_count=0
              impl_count=$(grep -c "^\\s*impl\\s" "$file" 2>/dev/null | tr -d '[:space:]') || impl_count=0
              
              # Default empty values to 0
              total_lines=''${total_lines:-0}
              fn_count=''${fn_count:-0}
              struct_count=''${struct_count:-0}
              impl_count=''${impl_count:-0}
              
              if [[ "$FIRST" == "true" ]]; then
                FIRST=false
              else
                echo "," >> "$TEMP_FILES"
              fi
              
              # Use jq to safely construct JSON with proper escaping
              jq -n \
                --arg path "$rel_path" \
                --argjson lines "$total_lines" \
                --argjson functions "$fn_count" \
                --argjson structs "$struct_count" \
                --argjson impl_blocks "$impl_count" \
                '{"path":$path,"lines":$lines,"functions":$functions,"structs":$structs,"impl_blocks":$impl_blocks}' \
                >> "$TEMP_FILES"
            fi
          done < "$OUTPUT_DIR/rust-files.txt"
          
          echo ']' >> "$TEMP_FILES"
          
          # Build final JSON
          jq -s '{"files":.[0]}' "$TEMP_FILES" > "$OUTPUT_DIR/file-metrics.json"
          rm -f "$TEMP_FILES"
          FILE_FAILURE_COUNT=$(jq '[.files[] | select(.lines > 500)] | length' "$OUTPUT_DIR/file-metrics.json")
          REPORT_STATUS=PASSED
          if [[ $ANALYSIS_FAILED -ne 0 ]]; then
            REPORT_STATUS=INCOMPLETE
          elif [[ $VIOLATION_COUNT -gt 0 || $FILE_FAILURE_COUNT -gt 0 ]]; then
            REPORT_STATUS=FAILED
          fi

          # Generate HTML report
          echo "🎨 Generating HTML report..."
          
          TOTAL_LOC=$(jq -r '.Rust.code // 0' "$OUTPUT_DIR/tokei-report.json")
          
          cat > "$OUTPUT_DIR/index.html" << EOF
      <!DOCTYPE html>
      <html>
      <head>
        <meta charset="UTF-8">
        <title>Code Complexity Report</title>
        <style>
          body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; max-width: 1200px; margin: 0 auto; padding: 20px; background: #f5f5f5; }
          h1 { color: #333; border-bottom: 3px solid #4a90d9; padding-bottom: 10px; }
          .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 15px; margin: 20px 0; }
          .metric-card { background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); text-align: center; }
          .metric-value { font-size: 2em; font-weight: bold; color: #4a90d9; }
          table { width: 100%; border-collapse: collapse; background: white; border-radius: 8px; overflow: hidden; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }
          th, td { padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }
          th { background: #4a90d9; color: white; font-weight: 600; }
          tr:hover { background: #f5f5f5; }
        </style>
      </head>
      <body>
        <h1>🔍 Code Complexity Report</h1>
        <p>Analysis status: $REPORT_STATUS. File size violations: $FILE_FAILURE_COUNT.</p>
        <div class="summary">
          <div class="metric-card"><div class="metric-value">$FILE_COUNT</div><div>Rust Files</div></div>
          <div class="metric-card"><div class="metric-value">$TOTAL_LOC</div><div>Lines of Code</div></div>
          <div class="metric-card"><div class="metric-value">$VIOLATION_COUNT</div><div>Violations</div></div>
        </div>
        <h2>📁 Per-File Metrics</h2>
        <table>
          <thead><tr><th>File</th><th>Lines</th><th>Functions</th><th>Structs</th><th>Impls</th></tr></thead>
          <tbody>
      EOF

          jq -r '.files | sort_by(.lines) | reverse | .[] | 
            "<tr><td>" + .path + "</td><td>" + (.lines | tostring) + "</td><td>" + 
            (.functions | tostring) + "</td><td>" + (.structs | tostring) + "</td><td>" + 
            (.impl_blocks | tostring) + "</td></tr>"' "$OUTPUT_DIR/file-metrics.json" >> "$OUTPUT_DIR/index.html"

          cat >> "$OUTPUT_DIR/index.html" << EOF
          </tbody>
        </table>
        <footer style="margin-top: 40px; padding-top: 20px; border-top: 1px solid #ddd; color: #666; text-align: center;">
          Generated on $(date -u '+%Y-%m-%d %H:%M UTC')
        </footer>
      </body>
      </html>
      EOF

          # Generate Markdown summary for MR comments
          echo "📝 Generating Markdown summary..."
          
          cat > "$OUTPUT_DIR/summary.md" << EOF
      ## 🔍 Code Complexity Report

      | Metric | Value |
      |--------|-------|
      | Rust Files | $FILE_COUNT |
      | Lines of Code | $TOTAL_LOC |
      | Complexity Violations | $VIOLATION_COUNT |

      ### 📁 Largest Files (Top 10)

      | File | Lines | Functions |
      |------|-------|-----------|
      EOF

          jq -r '.files | sort_by(.lines) | reverse | .[0:10] | .[] |
            "| " + .path + " | " + (.lines | tostring) + " | " + (.functions | tostring) + " |"' \
            "$OUTPUT_DIR/file-metrics.json" >> "$OUTPUT_DIR/summary.md"

          if [[ $ANALYSIS_FAILED -ne 0 ]]; then
            echo "### ❌ Status: INCOMPLETE" >> "$OUTPUT_DIR/summary.md"
            echo "Clippy failed; these metrics do not verify a successful analysis. See clippy-output.txt and clippy-status.json." >> "$OUTPUT_DIR/summary.md"
          elif [[ $VIOLATION_COUNT -eq 0 && $FILE_FAILURE_COUNT -eq 0 ]]; then
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "### ✅ Status: PASSED" >> "$OUTPUT_DIR/summary.md"
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "No complexity violations detected." >> "$OUTPUT_DIR/summary.md"
          else
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "### ❌ Status: FAILED" >> "$OUTPUT_DIR/summary.md"
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "Found $VIOLATION_COUNT complexity violations and $FILE_FAILURE_COUNT file size violations that need to be addressed." >> "$OUTPUT_DIR/summary.md"
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "<details>" >> "$OUTPUT_DIR/summary.md"
            echo "<summary>Click to view violations</summary>" >> "$OUTPUT_DIR/summary.md"
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo '```' >> "$OUTPUT_DIR/summary.md"
            head -20 "$OUTPUT_DIR/complexity-violations.txt" >> "$OUTPUT_DIR/summary.md"
            if [[ $VIOLATION_COUNT -gt 20 ]]; then
              echo "... and $((VIOLATION_COUNT - 20)) more violations" >> "$OUTPUT_DIR/summary.md"
            fi
            echo '```' >> "$OUTPUT_DIR/summary.md"
            echo "</details>" >> "$OUTPUT_DIR/summary.md"
          fi

          # Generate JUnit XML report
          echo "🧪 Generating JUnit XML report..."
          
          # Preserve the existing 500-line file clause. Counts describe actual
          # testcase elements; compiler failures are errors, not skipped tests.
          jq -nr --slurpfile files "$OUTPUT_DIR/file-metrics.json" \
            --slurpfile statuses "$OUTPUT_DIR/clippy-status.json" \
            --slurpfile violations "$OUTPUT_DIR/complexity-violations.json" '
            ($files[0].files) as $files | ($statuses[0]) as $statuses |
            ($violations[0]) as $violations |
            ($files | map(select(.lines > 500)) | length) as $large |
            ($statuses | map(select(.exit_status != 0)) | length) as $package_errors |
            ($package_errors + (if $package_errors > 0 then 1 else 0 end)) as $errors |
            (($files | length) + ($statuses | length) + 1) as $tests |
            ($large + (if ($violations | length) > 0 then 1 else 0 end)) as $failures |
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<testsuites name=\"Code Complexity\" tests=\"\($tests)\" failures=\"\($failures)\" errors=\"\($errors)\">",
            "<testsuite name=\"Complexity Checks\" tests=\"\($tests)\" failures=\"\($failures)\" errors=\"\($errors)\">",
            ($statuses[] | "<testcase classname=\"clippy\" name=\"\(.package | @html)\" time=\"\(.elapsed_seconds)\">" +
              (if .exit_status != 0 then "<error message=\"Clippy exited \(.exit_status); see diagnostic artifacts\"/>" else "" end) + "</testcase>"),
            "<testcase classname=\"complexity\" name=\"Clippy complexity lints\">" +
              (if $package_errors > 0 then "<error message=\"Analysis incomplete; Clippy failed\"/>" else "" end) +
              (if ($violations | length) > 0 then "<failure message=\"\($violations | length) complexity violations\">\($violations | tojson | @html)</failure>" else "" end) + "</testcase>",
            ($files[] | "<testcase classname=\"complexity\" name=\"\(.path | @html)\">" +
              (if .lines > 500 then "<failure message=\"File exceeds 500 lines\"/>" else "" end) + "</testcase>"),
            "</testsuite></testsuites>"' > "$OUTPUT_DIR/junit-report.xml"
          echo "File size clause: $FILE_FAILURE_COUNT files exceed 500 lines" >> "$OUTPUT_DIR/summary.md"
          if [[ $FILE_FAILURE_COUNT -gt 0 ]]; then
            echo "### ❌ File size status: FAILED" >> "$OUTPUT_DIR/summary.md"
          fi

          echo ""
          echo "✅ Complexity report generated!"
          echo "   HTML Report: $OUTPUT_DIR/index.html"
          echo "   Markdown Summary: $OUTPUT_DIR/summary.md"
          echo "   JUnit Report: $OUTPUT_DIR/junit-report.xml"

          if [[ $ANALYSIS_FAILED -ne 0 ]]; then
            echo "❌ Clippy analysis failed; report is incomplete"
            exit 1
          fi

          if [[ $VIOLATION_COUNT -gt 0 || $FILE_FAILURE_COUNT -gt 0 ]]; then
            echo ""
            echo "❌ Build failed due to $VIOLATION_COUNT complexity violations and $FILE_FAILURE_COUNT file size violations"
            exit 1
          fi

          exit 0
    '';
  };
in complexity-report // { inherit complexity-report; }
