{ pkgs, lib, ... }:
with lib;
with lib.crystal-forge;
let
  # Script to run complexity analysis and generate reports
  complexity-report = pkgs.writeShellScriptBin "complexity-report" ''
        set -euo pipefail

        PROJECT_ROOT="''${PROJECT_ROOT:-$(pwd)}"
        OUTPUT_DIR="''${OUTPUT_DIR:-$PROJECT_ROOT/complexity-report}"

        mkdir -p "$OUTPUT_DIR"

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

        # Run tokei for LOC statistics
        echo "📊 Generating LOC statistics..."
        ${pkgs.tokei}/bin/tokei --output json "$PROJECT_ROOT" > "$OUTPUT_DIR/tokei-report.json" 2>/dev/null || true

        # Run cargo clippy with complexity lints on all packages
        echo "🔎 Running clippy complexity lints..."

        CLIPPY_OUTPUT="$OUTPUT_DIR/clippy-output.txt"
        touch "$CLIPPY_OUTPUT"

        # List of packages to analyze
        PACKAGES=("packages/default" "packages/web-ui")

        # Export clippy lints via environment variable
        export RUSTFLAGS="-Wclippy::complexity -Wclippy::cognitive_complexity -Wclippy::too_many_arguments -Wclippy::too_many_lines -Wclippy::type_complexity -Wclippy::fn_params_excessive_bools -Wclippy::vec_box"

        for pkg in "''${PACKAGES[@]}"; do
          if [[ -d "$PROJECT_ROOT/$pkg" ]] && [[ -f "$PROJECT_ROOT/$pkg/Cargo.toml" ]]; then
            echo "   Analyzing $pkg..."
            cd "$PROJECT_ROOT/$pkg"
            
            # Run clippy with complexity lints (via RUSTFLAGS)
            ${pkgs.cargo}/bin/cargo clippy --all-targets --all-features --message-format=short 2>&1 | tee -a "$CLIPPY_OUTPUT" || true
          fi
        done

        cd "$PROJECT_ROOT"

        # Parse clippy output for violations
        echo "📋 Parsing complexity violations..."

        # Extract cognitive complexity violations
        ${pkgs.gnugrep}/bin/grep -E "cognitive_complexity|too_many_arguments|too_many_lines|type_complexity|fn_params_excessive_bools|vec_box" \
          "$CLIPPY_OUTPUT" > "$OUTPUT_DIR/complexity-violations.txt" 2>/dev/null || true

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
            total_lines=$(wc -l < "$file")
            fn_count=$(${pkgs.gnugrep}/bin/grep -c "^\\s*\\(pub\\s\\+\\)\\?\\(async\\s\\+\\)\\?fn\\s" "$file" 2>/dev/null || echo "0")
            struct_count=$(${pkgs.gnugrep}/bin/grep -c "^\\s*\\(pub\\s\\+\\)\\?struct\\s" "$file" 2>/dev/null || echo "0")
            impl_count=$(${pkgs.gnugrep}/bin/grep -c "^\\s*impl\\s" "$file" 2>/dev/null || echo "0")
            
            if [[ "$FIRST" == "true" ]]; then
              FIRST=false
            else
              echo "," >> "$TEMP_FILES"
            fi
            
            # Write JSON object on single line to avoid formatting issues
            printf '{"path":"%s","lines":%s,"functions":%s,"structs":%s,"impl_blocks":%s}' \
              "$rel_path" "$total_lines" "$fn_count" "$struct_count" "$impl_count" >> "$TEMP_FILES"
          fi
        done < "$OUTPUT_DIR/rust-files.txt"
        
        echo ']' >> "$TEMP_FILES"
        
        # Build final JSON
        ${pkgs.jq}/bin/jq -s '{"files":.[0]}' "$TEMP_FILES" > "$OUTPUT_DIR/file-metrics.json"
        rm -f "$TEMP_FILES"

        # Generate HTML report
        echo "🎨 Generating HTML report..."
        
        TOTAL_LOC=$(${pkgs.jq}/bin/jq -r '.Rust.code // 0' "$OUTPUT_DIR/tokei-report.json" 2>/dev/null || echo "0")
        
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

        ${pkgs.jq}/bin/jq -r '.files | sort_by(.lines) | reverse | .[] | 
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

        ${pkgs.jq}/bin/jq -r '.files | sort_by(.lines) | reverse | .[0:10] | .[] |
          "| " + .path + " | " + (.lines | tostring) + " | " + (.functions | tostring) + " |"' \
          "$OUTPUT_DIR/file-metrics.json" >> "$OUTPUT_DIR/summary.md"

        if [[ $VIOLATION_COUNT -eq 0 ]]; then
          echo "" >> "$OUTPUT_DIR/summary.md"
          echo "### ✅ Status: PASSED" >> "$OUTPUT_DIR/summary.md"
          echo "" >> "$OUTPUT_DIR/summary.md"
          echo "No complexity violations detected." >> "$OUTPUT_DIR/summary.md"
        else
          echo "" >> "$OUTPUT_DIR/summary.md"
          echo "### ❌ Status: FAILED" >> "$OUTPUT_DIR/summary.md"
          echo "" >> "$OUTPUT_DIR/summary.md"
          echo "Found $VIOLATION_COUNT complexity violations that need to be addressed." >> "$OUTPUT_DIR/summary.md"
          echo "" >> "$OUTPUT_DIR/summary.md"
          echo "<details>" >> "$OUTPUT_DIR/summary.md"
          echo "<summary>Click to view violations</summary>" >> "$OUTPUT_DIR/summary.md"
          echo "" >> "$OUTPUT_DIR/summary.md"
          echo '\`\`\`' >> "$OUTPUT_DIR/summary.md"
          head -20 "$OUTPUT_DIR/complexity-violations.txt" >> "$OUTPUT_DIR/summary.md"
          if [[ $VIOLATION_COUNT -gt 20 ]]; then
            echo "... and $((VIOLATION_COUNT - 20)) more violations" >> "$OUTPUT_DIR/summary.md"
          fi
          echo '\`\`\`' >> "$OUTPUT_DIR/summary.md"
          echo "</details>" >> "$OUTPUT_DIR/summary.md"
        fi

        # Generate JUnit XML report
        echo "🧪 Generating JUnit XML report..."
        
        cat > "$OUTPUT_DIR/junit-report.xml" << EOF
    <?xml version="1.0" encoding="UTF-8"?>
    <testsuites name="Code Complexity" tests="$FILE_COUNT" failures="$VIOLATION_COUNT">
      <testsuite name="Complexity Checks" tests="$FILE_COUNT" failures="$VIOLATION_COUNT">
    EOF

        ${pkgs.jq}/bin/jq -r '.files[] | 
          "<testcase classname=\"complexity\" name=\"" + .path + "\">" +
          if .lines > 500 then "<failure message=\"File exceeds 500 lines\"/>" else "" end +
          "</testcase>"' "$OUTPUT_DIR/file-metrics.json" >> "$OUTPUT_DIR/junit-report.xml"
        
        echo "  </testsuite>" >> "$OUTPUT_DIR/junit-report.xml"
        echo "</testsuites>" >> "$OUTPUT_DIR/junit-report.xml"

        echo ""
        echo "✅ Complexity report generated!"
        echo "   HTML Report: $OUTPUT_DIR/index.html"
        echo "   Markdown Summary: $OUTPUT_DIR/summary.md"
        echo "   JUnit Report: $OUTPUT_DIR/junit-report.xml"

        if [[ $VIOLATION_COUNT -gt 0 ]]; then
          echo ""
          echo "❌ Build failed due to $VIOLATION_COUNT complexity violations"
          exit 1
        fi

        exit 0
  '';
in complexity-report // { inherit complexity-report; }
