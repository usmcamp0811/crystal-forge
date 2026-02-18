{ pkgs, lib, ... }:
with lib;
with lib.crystal-forge;
let
  # Script to run code coverage analysis and generate reports
  coverage-report = pkgs.writeShellScriptBin "coverage-report" ''
        set -euo pipefail

        PROJECT_ROOT="''${PROJECT_ROOT:-$(pwd)}"
        OUTPUT_DIR="''${OUTPUT_DIR:-$PROJECT_ROOT/coverage-report}"
        TARGET_BRANCH="''${CI_MERGE_REQUEST_TARGET_BRANCH_NAME:-main}"

        mkdir -p "$OUTPUT_DIR"

        echo "📊 Analyzing code coverage..."
        echo "   Project root: $PROJECT_ROOT"
        echo "   Output dir: $OUTPUT_DIR"

        # Initialize JSON for coverage data
        echo '{"packages":[]}' > "$OUTPUT_DIR/coverage.json"

        # List of packages to analyze
        PACKAGES=("packages/default")
        
        TOTAL_LINE_COUNT=0
        TOTAL_COVERED_COUNT=0
        TOTAL_PACKAGE_COUNT=0

        for pkg in "''${PACKAGES[@]}"; do
          if [[ -d "$PROJECT_ROOT/$pkg" ]] && [[ -f "$PROJECT_ROOT/$pkg/Cargo.toml" ]]; then
            echo ""
            echo "🔍 Analyzing $pkg..."
            cd "$PROJECT_ROOT/$pkg"
            
            PKG_OUTPUT="$OUTPUT_DIR/$pkg"
            mkdir -p "$PKG_OUTPUT"
            
            # Run tarpaulin with JSON and HTML output
            echo "   Running cargo tarpaulin..."
            if ${pkgs.cargo-tarpaulin}/bin/cargo tarpaulin \
              --out Html \
              --out Json \
              --output-dir "$PKG_OUTPUT" \
              --all-features \
              --workspace \
              --timeout 300 \
              2>&1 | tee "$PKG_OUTPUT/tarpaulin.log"; then
              
              echo "   ✓ Coverage analysis complete for $pkg"
              
              # Parse JSON coverage report
              if [[ -f "$PKG_OUTPUT/tarpaulin-report.json" ]]; then
                # Extract coverage data
                LINE_COUNT=$(${pkgs.jq}/bin/jq -r '.files | map(.line_count) | add // 0' "$PKG_OUTPUT/tarpaulin-report.json")
                COVERED_COUNT=$(${pkgs.jq}/bin/jq -r '.files | map(.covered) | add // 0' "$PKG_OUTPUT/tarpaulin-report.json")
                
                if [[ $LINE_COUNT -gt 0 ]]; then
                  COVERAGE_PCT=$(echo "scale=2; $COVERED_COUNT * 100 / $LINE_COUNT" | ${pkgs.bc}/bin/bc)
                else
                  COVERAGE_PCT="0.00"
                fi
                
                echo "   Coverage: $COVERAGE_PCT% ($COVERED_COUNT/$LINE_COUNT lines)"
                
                # Add to totals
                TOTAL_LINE_COUNT=$((TOTAL_LINE_COUNT + LINE_COUNT))
                TOTAL_COVERED_COUNT=$((TOTAL_COVERED_COUNT + COVERED_COUNT))
                TOTAL_PACKAGE_COUNT=$((TOTAL_PACKAGE_COUNT + 1))
                
                # Add to coverage.json
                ${pkgs.jq}/bin/jq --arg pkg "$pkg" \
                  --argjson lines "$LINE_COUNT" \
                  --argjson covered "$COVERED_COUNT" \
                  --argjson pct "$COVERAGE_PCT" \
                  '.packages += [{"name": $pkg, "lines": $lines, "covered": $covered, "coverage": $pct}]' \
                  "$OUTPUT_DIR/coverage.json" > "$OUTPUT_DIR/coverage.json.tmp"
                mv "$OUTPUT_DIR/coverage.json.tmp" "$OUTPUT_DIR/coverage.json"
              fi
              
              # Copy HTML report to main output
              if [[ -f "$PKG_OUTPUT/tarpaulin-report.html" ]]; then
                cp "$PKG_OUTPUT/tarpaulin-report.html" "$OUTPUT_DIR/$pkg-coverage.html"
              fi
            else
              echo "   ⚠️ Coverage analysis failed for $pkg (tests may have failed)"
            fi
          fi
        done

        cd "$PROJECT_ROOT"

        # Calculate overall coverage
        if [[ $TOTAL_LINE_COUNT -gt 0 ]]; then
          OVERALL_COVERAGE=$(echo "scale=2; $TOTAL_COVERED_COUNT * 100 / $TOTAL_LINE_COUNT" | ${pkgs.bc}/bin/bc)
        else
          OVERALL_COVERAGE="0.00"
        fi

        echo ""
        echo "📈 Overall Coverage: $OVERALL_COVERAGE% ($TOTAL_COVERED_COUNT/$TOTAL_LINE_COUNT lines)"

        # Update coverage.json with summary
        ${pkgs.jq}/bin/jq --argjson totalLines "$TOTAL_LINE_COUNT" \
          --argjson totalCovered "$TOTAL_COVERED_COUNT" \
          --argjson overall "$OVERALL_COVERAGE" \
          '. + {"summary": {"total_lines": $totalLines, "total_covered": $totalCovered, "overall_coverage": $overall}}' \
          "$OUTPUT_DIR/coverage.json" > "$OUTPUT_DIR/coverage.json.tmp"
        mv "$OUTPUT_DIR/coverage.json.tmp" "$OUTPUT_DIR/coverage.json"

        # Generate main HTML index
        echo "🎨 Generating HTML index..."
        
        cat > "$OUTPUT_DIR/index.html" << EOF
    <!DOCTYPE html>
    <html>
    <head>
      <meta charset="UTF-8">
      <title>Code Coverage Report</title>
      <style>
        body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; max-width: 1200px; margin: 0 auto; padding: 20px; background: #f5f5f5; }
        h1 { color: #333; border-bottom: 3px solid #28a745; padding-bottom: 10px; }
        .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(250px, 1fr)); gap: 15px; margin: 20px 0; }
        .metric-card { background: white; padding: 20px; border-radius: 8px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); text-align: center; }
        .metric-value { font-size: 2.5em; font-weight: bold; color: #28a745; }
        .metric-value.warning { color: #ffc107; }
        .metric-value.danger { color: #dc3545; }
        .metric-label { color: #666; margin-top: 5px; }
        .packages { background: white; padding: 20px; border-radius: 8px; margin: 20px 0; }
        table { width: 100%; border-collapse: collapse; }
        th, td { padding: 12px; text-align: left; border-bottom: 1px solid #ddd; }
        th { background: #28a745; color: white; font-weight: 600; }
        tr:hover { background: #f5f5f5; }
        .coverage-bar { background: #e9ecef; border-radius: 4px; height: 20px; overflow: hidden; }
        .coverage-fill { background: #28a745; height: 100%; }
        a { color: #28a745; text-decoration: none; }
        a:hover { text-decoration: underline; }
      </style>
    </head>
    <body>
      <h1>📊 Code Coverage Report</h1>
      
      <div class="summary">
        <div class="metric-card">
          <div class="metric-value">$OVERALL_COVERAGE%</div>
          <div class="metric-label">Overall Coverage</div>
        </div>
        <div class="metric-card">
          <div class="metric-value">$TOTAL_COVERED_COUNT</div>
          <div class="metric-label">Lines Covered</div>
        </div>
        <div class="metric-card">
          <div class="metric-value">$TOTAL_LINE_COUNT</div>
          <div class="metric-label">Total Lines</div>
        </div>
        <div class="metric-card">
          <div class="metric-value">$TOTAL_PACKAGE_COUNT</div>
          <div class="metric-label">Packages Analyzed</div>
        </div>
      </div>

      <div class="packages">
        <h2>📁 Package Coverage</h2>
        <table>
          <thead>
            <tr>
              <th>Package</th>
              <th>Coverage</th>
              <th>Lines</th>
              <th>Covered</th>
              <th>Report</th>
            </tr>
          </thead>
          <tbody>
    EOF

        # Add package rows
        ${pkgs.jq}/bin/jq -r '.packages[] | 
          "<tr>" +
          "<td>" + .name + "</td>" +
          "<td><div class=\"coverage-bar\"><div class=\"coverage-fill\" style=\"width: " + (.coverage | tostring) + "%\"></div></div>" + (.coverage | tostring) + "%</td>" +
          "<td>" + (.lines | tostring) + "</td>" +
          "<td>" + (.covered | tostring) + "</td>" +
          "<td><a href=\"" + (.name | gsub("/"; "-")) + "-coverage.html\">View Report</a></td>" +
          "</tr>"' "$OUTPUT_DIR/coverage.json" >> "$OUTPUT_DIR/index.html"

        cat >> "$OUTPUT_DIR/index.html" << EOF
          </tbody>
        </table>
      </div>

      <footer style="margin-top: 40px; padding-top: 20px; border-top: 1px solid #ddd; color: #666; text-align: center;">
        Generated on $(date -u '+%Y-%m-%d %H:%M UTC')<br>
        Generated by <a href="https://github.com/xd009642/tarpaulin">cargo-tarpaulin</a>
      </footer>
    </body>
    </html>
    EOF

        # Generate Markdown summary for MR comments
        echo "📝 Generating Markdown summary..."
        
        cat > "$OUTPUT_DIR/summary.md" << EOF
    ## 📊 Code Coverage Report

    | Metric | Value |
    |--------|-------|
    | Overall Coverage | **$OVERALL_COVERAGE%** |
    | Lines Covered | $TOTAL_COVERED_COUNT |
    | Total Lines | $TOTAL_LINE_COUNT |
    | Packages Analyzed | $TOTAL_PACKAGE_COUNT |

    ### 📁 Package Coverage Details

    | Package | Coverage | Lines | Covered |
    |---------|----------|-------|---------|
    EOF

        ${pkgs.jq}/bin/jq -r '.packages[] |
          "| " + .name + " | " + (.coverage | tostring) + "% | " + (.lines | tostring) + " | " + (.covered | tostring) + " |"' \
          "$OUTPUT_DIR/coverage.json" >> "$OUTPUT_DIR/summary.md"

        # Add coverage badge/status
        if (( $(echo "$OVERALL_COVERAGE >= 80" | ${pkgs.bc}/bin/bc -l) )); then
          echo -e "\n### ✅ Coverage Status: GOOD\n\nOverall coverage is above 80%." >> "$OUTPUT_DIR/summary.md"
        elif (( $(echo "$OVERALL_COVERAGE >= 60" | ${pkgs.bc}/bin/bc -l) )); then
          echo -e "\n### ⚠️ Coverage Status: MODERATE\n\nOverall coverage is between 60-80%. Consider adding more tests." >> "$OUTPUT_DIR/summary.md"
        else
          echo -e "\n### ❌ Coverage Status: LOW\n\nOverall coverage is below 60%. Additional tests are recommended." >> "$OUTPUT_DIR/summary.md"
        fi

        # Generate JUnit XML report
        echo "🧪 Generating JUnit XML report..."
        
        cat > "$OUTPUT_DIR/junit-report.xml" << EOF
    <?xml version="1.0" encoding="UTF-8"?>
    <testsuites name="Code Coverage" tests="$TOTAL_PACKAGE_COUNT" failures="0">
      <testsuite name="Coverage Analysis" tests="$TOTAL_PACKAGE_COUNT" failures="0">
    EOF

        ${pkgs.jq}/bin/jq -r '.packages[] |
          "<testcase classname=\"coverage\" name=\"" + .name + "\" time=\"0\">" +
          "<system-out>Coverage: " + (.coverage | tostring) + "% (" + (.covered | tostring) + "/" + (.lines | tostring) + " lines)</system-out>" +
          if (.coverage | tonumber) < 60 then "<failure message=\"Coverage below 60%\"/>" else "" end +
          "</testcase>"' "$OUTPUT_DIR/coverage.json" >> "$OUTPUT_DIR/junit-report.xml"
        
        echo "  </testsuite>" >> "$OUTPUT_DIR/junit-report.xml"
        echo "</testsuites>" >> "$OUTPUT_DIR/junit-report.xml"

        echo ""
        echo "✅ Coverage report generated!"
        echo "   HTML Report: $OUTPUT_DIR/index.html"
        echo "   Markdown Summary: $OUTPUT_DIR/summary.md"
        echo "   JSON Data: $OUTPUT_DIR/coverage.json"
        echo "   JUnit Report: $OUTPUT_DIR/junit-report.xml"

        # Warn if coverage is low (but don't fail)
        if (( $(echo "$OVERALL_COVERAGE < 60" | ${pkgs.bc}/bin/bc -l) )); then
          echo ""
          echo "⚠️ Warning: Overall coverage ($OVERALL_COVERAGE%) is below 60%"
        fi

        exit 0
  '';
in coverage-report // { inherit coverage-report; }
