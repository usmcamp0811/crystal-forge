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

          PROJECT_ROOT=$(realpath "''${PROJECT_ROOT:-$(pwd)}")
          OUTPUT_DIR=$(realpath -m "''${OUTPUT_DIR:-$PROJECT_ROOT/complexity-report}")

          mkdir -p "$OUTPUT_DIR"
          rm -f "$OUTPUT_DIR"/{index.html,summary.md,junit-report.xml,analysis-summary.json,rust-files.txt,tokei-report.json,file-metrics.json,complexity-violations.txt,complexity-violations.json,clippy-output.txt,clippy-messages.jsonl,clippy-status.json}
          rm -f "$OUTPUT_DIR"/packages-{default,web-ui}.{jsonl,stderr}
          trap 'status=$?; if [[ $status -ne 0 ]]; then rm -f "$OUTPUT_DIR"/{index.html,summary.md}; printf "<testsuites tests=\"1\" errors=\"1\"><testsuite name=\"Complexity analysis\" tests=\"1\" errors=\"1\"><testcase name=\"Required analysis\"><error message=\"Analysis incomplete or failed; see diagnostic artifacts\"/></testcase></testsuite></testsuites>\n" > "$OUTPUT_DIR/junit-report.xml"; fi' EXIT
          export RUSTC="${pkgs.rustc}/bin/rustc"
          export RUSTDOC="${pkgs.rustc}/bin/rustdoc"
          # Native all-features analysis needs the same development outputs as
          # packaging. SQLx uses checked-in metadata, never a live database.
          export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkgconfig''${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
          export OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib"
          export OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include"
          export SQLX_OFFLINE=true
          export CRYSTAL_FORGE_UI_DIST="$PROJECT_ROOT/packages/web-ui/assets"
          export CARGO_BUILD_JOBS=2
          export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0

          echo "🔍 Analyzing code complexity..."
          echo "   Project root: $PROJECT_ROOT"
          echo "   Output dir: $OUTPUT_DIR"

          # Find all Rust source files (excluding tests, generated, target)
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
          [[ $FILE_COUNT -gt 0 ]] || { echo "No Rust sources measured" >&2; exit 1; }

          # Run tokei for LOC statistics
          echo "📊 Generating LOC statistics..."
          tokei --output json "$PROJECT_ROOT" > "$OUTPUT_DIR/tokei-report.json"
          jq -se 'length == 1 and (.[0].Rust.code | type == "number" and . > 0 and floor == .)' "$OUTPUT_DIR/tokei-report.json" > /dev/null

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
            status=0
            package_log="$OUTPUT_DIR/''${pkg//\//-}"
            : > "$package_log.jsonl"
            : > "$package_log.stderr"
            if [[ -f "$PROJECT_ROOT/$pkg/Cargo.toml" ]]; then
              echo "   Analyzing $pkg..."
              cd "$PROJECT_ROOT/$pkg"
              
              # Run clippy with complexity lints (via RUSTFLAGS)
              cargo clippy --all-targets --all-features --message-format=json \
                > "$package_log.jsonl" 2> "$package_log.stderr" || status=$?
            else
              echo "Missing required manifest: $pkg/Cargo.toml" > "$package_log.stderr"
              status=1
            fi
            cat "$package_log.stderr" | tee -a "$CLIPPY_OUTPUT"
            # A successful process is insufficient: Cargo must emit complete,
            # valid JSON and finish successfully. Reject empty/partial streams.
            if ! jq -se 'length > 0 and all(.[]; type == "object" and (.reason | type == "string")) and
              all(.[]; .reason != "compiler-message" or
                ((.message | type == "object") and (.message.level | type == "string") and
                 (.message.message | type == "string") and (.message.spans | type == "array") and
                 (.message.code == null or (.message.code.code | type == "string")))) and
              ([.[] | select(.reason == "build-finished")] | length == 1) and
              .[-1].reason == "build-finished" and .[-1].success == true and
              any(.[]; .reason == "compiler-artifact") and
              all(.[]; .reason != "compiler-message" or
                (.message.level != "error" and .message.code.code? != "unknown_lints"))' "$package_log.jsonl" > /dev/null; then
              if [[ $status -eq 0 ]]; then status=1; fi
            fi
            if jq -e -s '.' "$package_log.jsonl" > /dev/null; then
              cat "$package_log.jsonl" >> "$OUTPUT_DIR/clippy-messages.jsonl"
              jq -r 'select(.reason == "compiler-message") | .message.rendered // .message.message' \
                "$package_log.jsonl" >> "$CLIPPY_OUTPUT"
            fi
            jq --arg package "$pkg" --argjson status "$status" \
              '. + [{package: $package, exit_status: $status}]' "$OUTPUT_DIR/clippy-status.json" > "$OUTPUT_DIR/clippy-status.json.tmp"
            mv "$OUTPUT_DIR/clippy-status.json.tmp" "$OUTPUT_DIR/clippy-status.json"
            if [[ $status -ne 0 ]]; then ANALYSIS_FAILED=1; fi
          done

          cd "$PROJECT_ROOT"

          # Parse clippy output for violations
          echo "📋 Parsing complexity violations..."

          # Match the existing six diagnostic codes, never prose/help links.
          # All-targets can repeat the same diagnostic; count it once.
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
              total_lines=$(wc -l < "$file" | tr -d '[:space:]')
              # grep status 1 means a measured zero. Read/tool errors must fail.
              fn_count=$(grep -c "^\\s*\\(pub\\s\\+\\)\\?\\(async\\s\\+\\)\\?fn\\s" "$file") || [[ $? -eq 1 ]]
              struct_count=$(grep -c "^\\s*\\(pub\\s\\+\\)\\?struct\\s" "$file") || [[ $? -eq 1 ]]
              impl_count=$(grep -c "^\\s*impl\\s" "$file") || [[ $? -eq 1 ]]
              
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
            else
              echo "Measured source disappeared: $file" >&2
              exit 1
            fi
          done < "$OUTPUT_DIR/rust-files.txt"
          
          echo ']' >> "$TEMP_FILES"
          
          # Build final JSON
          jq -s '{"files":.[0]}' "$TEMP_FILES" > "$OUTPUT_DIR/file-metrics.json"
          rm -f "$TEMP_FILES"
          FILE_FAILURE_COUNT=$(jq '[.files[] | select(.lines > 500)] | length' "$OUTPUT_DIR/file-metrics.json")
          FUNCTION_COUNT=$(jq '[.files[].functions] | add' "$OUTPUT_DIR/file-metrics.json")
          REPORT_STATUS=COMPLETE
          DEBT_STATUS=NO_SELECTED_DEBT
          if [[ $ANALYSIS_FAILED -ne 0 ]]; then
            REPORT_STATUS=INCOMPLETE
            DEBT_STATUS=PARTIAL_MEASUREMENT
          elif [[ $VIOLATION_COUNT -gt 0 || $FILE_FAILURE_COUNT -gt 0 ]]; then
            REPORT_STATUS=COMPLETE_WITH_DEBT
            DEBT_STATUS=MEASURED_DEBT
          fi
          # MR !335 owns execution enforcement only. TASK-470 commit 83a9234b
          # defers strict quality gates; measurements never waive compiler errors.
          jq -n --arg status "$REPORT_STATUS" --arg debt "$DEBT_STATUS" \
            --argjson files "$FILE_COUNT" --argjson functions "$FUNCTION_COUNT" \
            --argjson findings "$VIOLATION_COUNT" --argjson oversized "$FILE_FAILURE_COUNT" \
            '{analysis_status: $status, debt_status: $debt,
              enforcement: "execution-only", policy_source: "TASK-470/83a9234b; MR335 owner decision",
              files: $files, functions: $functions, selected_lint_findings: $findings,
              files_over_500_lines: $oversized, thresholds_blocking: false,
              thresholds: {file_lines: 500,
                clippy: "Pinned Clippy configuration; measured values and limits in complexity-violations.json diagnostics"},
              scope: {packages: ["packages/default", "packages/web-ui"],
                targets: "native --all-targets --all-features",
                selected_codes: ["clippy::cognitive_complexity", "clippy::too_many_arguments",
                  "clippy::too_many_lines", "clippy::type_complexity",
                  "clippy::fn_params_excessive_bools", "clippy::vec_box"]}}' \
            > "$OUTPUT_DIR/analysis-summary.json"

          # Generate HTML report
          echo "🎨 Generating HTML report..."
          
          TOTAL_LOC=$(jq -r '.Rust.code' "$OUTPUT_DIR/tokei-report.json")
          
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
        <p>Analysis status: $REPORT_STATUS. Debt status: $DEBT_STATUS.</p>
        <p>Execution-only enforcement (MR335; TASK-470/83a9234b). Quality thresholds are advisory.</p>
        <p>$FUNCTION_COUNT function declarations measured. $FILE_FAILURE_COUNT files exceed 500 lines.
        Clippy measurements and configured limits are in complexity-violations.json.</p>
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
      | Function Declarations | $FUNCTION_COUNT |
      | Files Over 500 Lines (Advisory) | $FILE_FAILURE_COUNT |

      ### 📁 Largest Files (Top 10)

      | File | Lines | Functions |
      |------|-------|-----------|
      EOF

          jq -r '.files | sort_by(.lines) | reverse | .[0:10] | .[] |
            "| " + .path + " | " + (.lines | tostring) + " | " + (.functions | tostring) + " |"' \
            "$OUTPUT_DIR/file-metrics.json" >> "$OUTPUT_DIR/summary.md"

          if [[ $ANALYSIS_FAILED -ne 0 ]]; then
            echo "### ❌ Status: INCOMPLETE" >> "$OUTPUT_DIR/summary.md"
            echo "Clippy failed or emitted incomplete output. See clippy-status.json and diagnostic artifacts." >> "$OUTPUT_DIR/summary.md"
          elif [[ $VIOLATION_COUNT -eq 0 && $FILE_FAILURE_COUNT -eq 0 ]]; then
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "### Status: COMPLETE" >> "$OUTPUT_DIR/summary.md"
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "Analysis completed with no findings in the selected advisory clauses." >> "$OUTPUT_DIR/summary.md"
          else
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "### Status: COMPLETE_WITH_DEBT" >> "$OUTPUT_DIR/summary.md"
            echo "" >> "$OUTPUT_DIR/summary.md"
            echo "Measured $VIOLATION_COUNT selected Clippy findings and $FILE_FAILURE_COUNT files over 500 lines. These measurements are nonblocking under the owner-approved execution-only policy." >> "$OUTPUT_DIR/summary.md"
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
          echo "Debt status: $DEBT_STATUS. MR335 enforces execution only; TASK-470/83a9234b defers strict quality gates." >> "$OUTPUT_DIR/summary.md"
          echo "Thresholds: file lines >500; pinned Clippy limits and measured values appear in complexity-violations.json. Source declaration counts are lexical measurements, not a Rust syntax-tree inventory." >> "$OUTPUT_DIR/summary.md"

          # Generate JUnit XML report
          echo "🧪 Generating JUnit XML report..."
          
          # JUnit errors represent failed/incomplete execution. Advisory quality
          # measurements remain system-out data, never failures or skipped tests.
          jq -nr --slurpfile files "$OUTPUT_DIR/file-metrics.json" \
            --slurpfile statuses "$OUTPUT_DIR/clippy-status.json" \
            --slurpfile violations "$OUTPUT_DIR/complexity-violations.json" '
            ($files[0].files) as $files | ($statuses[0]) as $statuses |
            ($violations[0]) as $violations |
            ($files | map(select(.lines > 500)) | length) as $large |
            ($statuses | map(select(.exit_status != 0)) | length) as $errors |
            (($files | length) + ($statuses | length) + 1) as $tests |
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<testsuites name=\"Code Complexity\" tests=\"\($tests)\" failures=\"0\" errors=\"\($errors)\">",
            "<testsuite name=\"Complexity Checks\" tests=\"\($tests)\" failures=\"0\" errors=\"\($errors)\">",
            ($statuses[] | "<testcase classname=\"clippy\" name=\"\(.package | @html)\">" +
              (if .exit_status != 0 then "<error message=\"Clippy failed or output incomplete\"/>" else "" end) + "</testcase>"),
            "<testcase classname=\"complexity\" name=\"Clippy complexity lints\">" +
              "<system-out>Advisory selected findings: \($violations | length). \($violations | tojson | @html)</system-out></testcase>",
            ($files[] | "<testcase classname=\"complexity\" name=\"\(.path | @html)\">" +
              "<system-out>Lines: \(.lines); function declarations: \(.functions); advisory file threshold: 500 lines</system-out></testcase>"),
            "</testsuite></testsuites>"' > "$OUTPUT_DIR/junit-report.xml"

          echo ""
          echo "✅ Complexity report generated!"
          echo "   HTML Report: $OUTPUT_DIR/index.html"
          echo "   Markdown Summary: $OUTPUT_DIR/summary.md"
          echo "   JUnit Report: $OUTPUT_DIR/junit-report.xml"

          # Retain reports from both complete analysis and compiler failures.
          trap - EXIT
          if [[ $ANALYSIS_FAILED -ne 0 ]]; then exit 1; fi

          exit 0
    '';
  };
in complexity-report // { inherit complexity-report; }
