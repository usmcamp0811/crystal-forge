#!/usr/bin/env bash
# ── Crystal Forge Design Example – Local Server ──────────────────────────────
# Run this from the repository root (or from docs/design/CrystalForge) to
# preview the design example in your browser.
#
# Usage:
#   bash docs/design/CrystalForge/serve.sh
#
# Then open http://localhost:8080/crystal-forge.html?view=dashboard&theme=dark
# in your browser.
#
# The design app reads the shared golden fixture
# (docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json) to produce
# deterministic, database-independent UI mockups.
#
# Supported query params:
#   view   – any view name from the design-parity manifest
#            (dashboard, systems, builds, evaluations, flakes,
#             environments, caches, builders, policies, compliance,
#             cves, scanning, admin)
#   theme  – dark (default) or light
#
# Examples:
#   http://localhost:8080/crystal-forge.html?view=systems&theme=light
#   http://localhost:8080/crystal-forge.html?view=flakes&theme=dark
#   http://localhost:8080/crystal-forge.html?view=admin&theme=light
# ──────────────────────────────────────────────────────────────────────────────
set -euo pipefail

# Determine the design directory: either the script's location or the repo path.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DESIGN_DIR="$SCRIPT_DIR"

if [ ! -f "$DESIGN_DIR/crystal-forge.html" ]; then
  echo "ERROR: crystal-forge.html not found in $DESIGN_DIR"
  echo "Run this script from the repository root or from docs/design/CrystalForge/"
  exit 1
fi

PORT="${1:-8080}"

echo "────────────────────────────────────────────────────────────"
echo "  Crystal Forge Design Example"
echo "  Serving: $DESIGN_DIR"
echo "  URL:     http://localhost:$PORT/crystal-forge.html?view=dashboard&theme=dark"
echo ""
echo "  Open the above URL in your browser to verify the design."
echo "  Change ?view=dashboard to any view name from the manifest."
echo "  Change &theme=dark to &theme=light for light mode."
echo "────────────────────────────────────────────────────────────"

cd "$DESIGN_DIR"
python3 -m http.server "$PORT"
