/**
 * Design-parity comparison.
 *
 * Compares each design-example target screenshot (<view>--<theme>.design.png)
 * against the corresponding real Dioxus screenshot (<view>--<theme>.dioxus.png)
 * and writes a NON-BLOCKING design-drift report + a side-by-side montage per
 * view/theme so drift is visible in the MR.
 *
 * The two renderers (React design example vs Dioxus app) are never expected to
 * be pixel-identical, so we normalize both screenshots (resize to a common
 * width, flatten onto an opaque background) and use ImageMagick
 * `compare -metric RMSE` to produce a stable similarity gauge. Lower drift =
 * closer to the design.
 *
 * Usage:
 *   node compare-design-parity.js <manifest> <designDir> <dioxusDir> <outDir>
 *
 *   <designDir>  Directory with <view>--<theme>.design.png (targets).
 *   <dioxusDir>  Directory with <view>--<theme>.dioxus.png (real UI captures).
 *   <outDir>     Where the report, summary, and montages are written.
 */
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");

function sh(cmd) {
  return execSync(cmd, { encoding: "utf8", shell: "/bin/sh" }).trim();
}

function normalize(src, dst, width) {
  // Resize to a common width and flatten onto an opaque background so
  // transparency / height differences do not dominate the metric.
  sh(
    `convert "${src}" -resize ${width}x -background white -flatten "${dst}" 2>&1 || true`,
  );
  return fs.existsSync(dst);
}

function rmse(a, b) {
  // compare -metric RMSE prints "<abs> (<normalized>)"; the normalized value in
  // parentheses is in [0,1]. compare exits non-zero when images differ, so
  // tolerate that and parse the number.
  let out;
  try {
    out = sh(`compare -metric RMSE "${a}" "${b}" null: 2>&1 || true`);
  } catch (err) {
    out = String(err.stdout || err.message || "");
  }
  const m = out.match(/\(([0-9.eE+-]+)\)/);
  if (m) {
    const v = Number.parseFloat(m[1]);
    if (Number.isFinite(v)) return v;
  }
  return null;
}

function main() {
  const manifestPath = process.argv[2];
  const designDir = process.argv[3];
  const dioxusDir = process.argv[4];
  const outDir = process.argv[5] || "/tmp/design-parity";

  if (!manifestPath || !designDir || !dioxusDir) {
    console.error(
      "usage: node compare-design-parity.js <manifest> <designDir> <dioxusDir> <outDir>",
    );
    process.exit(2);
  }

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const themes = manifest.settings.themes || ["dark", "light"];
  const width = (manifest.settings.compare && manifest.settings.compare.resizeWidth) || 960;

  const normDir = path.join(outDir, "normalized");
  const montageDir = path.join(outDir, "montages");
  fs.mkdirSync(normDir, { recursive: true });
  fs.mkdirSync(montageDir, { recursive: true });

  const rows = [];
  for (const view of manifest.views) {
    for (const theme of themes) {
      const name = `${view.name}--${theme}`;
      const designPng = path.join(designDir, `${name}.design.png`);
      const dioxusPng = path.join(dioxusDir, `${name}.dioxus.png`);

      const hasDesign = fs.existsSync(designPng);
      const hasDioxus = fs.existsSync(dioxusPng);

      const row = {
        name,
        view: view.name,
        theme,
        route: view.route,
        designRef: view.designRef || null,
        hasDesign,
        hasDioxus,
        drift: null,
        similarity: null,
        status: "missing",
      };

      if (hasDesign && hasDioxus) {
        const nd = path.join(normDir, `${name}.design.png`);
        const nx = path.join(normDir, `${name}.dioxus.png`);
        const okD = normalize(designPng, nd, width);
        const okX = normalize(dioxusPng, nx, width);
        if (okD && okX) {
          const score = rmse(nd, nx);
          if (score !== null) {
            row.drift = Number(score.toFixed(4));
            row.similarity = Number((1 - score).toFixed(4));
            row.status = "compared";
            // Side-by-side montage for MR review.
            try {
              sh(
                `montage "${nd}" "${nx}" -tile 2x1 -geometry +4+4 -background '#111827' "${path.join(
                  montageDir,
                  `${name}.montage.png`,
                )}" 2>&1 || true`,
              );
            } catch (_) {}
          } else {
            row.status = "compare-error";
          }
        } else {
          row.status = "normalize-error";
        }
      }

      rows.push(row);
    }
  }

  const compared = rows.filter((r) => r.status === "compared");
  const avgDrift = compared.length
    ? Number(
        (compared.reduce((a, r) => a + r.drift, 0) / compared.length).toFixed(4),
      )
    : null;
  const avgSimilarity = avgDrift !== null ? Number((1 - avgDrift).toFixed(4)) : null;
  const worst = compared
    .slice()
    .sort((a, b) => b.drift - a.drift)
    .slice(0, 5)
    .map((r) => ({ name: r.name, drift: r.drift }));

  const report = {
    version: 1,
    blocking: false,
    themes,
    counts: {
      views: manifest.views.length,
      compared: compared.length,
      missing: rows.filter((r) => r.status === "missing").length,
      errors: rows.filter((r) => r.status.endsWith("error")).length,
    },
    avgDrift,
    avgSimilarity,
    worst,
    rows,
  };

  fs.mkdirSync(outDir, { recursive: true });
  fs.writeFileSync(
    path.join(outDir, "design-drift-report.json"),
    JSON.stringify(report, null, 2),
  );

  // Markdown summary consumed by the MR-comment CI job.
  const md = [];
  md.push("## Design Parity (non-blocking)");
  md.push(
    "Directional gauge of how closely the real Dioxus UI matches the tracked " +
      "design example under `docs/design/CrystalForge`, using the shared golden " +
      "fixture. Lower drift = closer to the design. This never fails the check.",
  );
  if (avgSimilarity !== null) {
    md.push(
      `**Overall design similarity:** ${(avgSimilarity * 100).toFixed(1)}% ` +
        `(avg drift ${avgDrift}) across ${compared.length} view/theme captures.`,
    );
  } else {
    md.push("**Overall design similarity:** no comparable captures were produced.");
  }
  if (worst.length) {
    md.push(
      "**Highest drift:** " +
        worst.map((w) => `\`${w.name}\` (${w.drift})`).join(", "),
    );
  }
  const missing = rows.filter((r) => r.status !== "compared");
  if (missing.length) {
    md.push(
      "**Not compared:** " +
        missing.map((r) => `\`${r.name}\` (${r.status})`).join(", "),
    );
  }
  md.push(
    "Per-view table:\n\n" +
      "| view | theme | drift | similarity | status |\n" +
      "| --- | --- | --- | --- | --- |\n" +
      rows
        .map(
          (r) =>
            `| ${r.view} | ${r.theme} | ${r.drift ?? "—"} | ${
              r.similarity !== null ? `${(r.similarity * 100).toFixed(1)}%` : "—"
            } | ${r.status} |`,
        )
        .join("\n"),
  );
  fs.writeFileSync(path.join(outDir, "design-drift-summary.md"), md.join("\n\n") + "\n");

  console.log("=== Design Parity ===");
  console.log(
    `  Compared: ${compared.length}/${rows.length}  avgDrift=${avgDrift}  avgSimilarity=${
      avgSimilarity !== null ? (avgSimilarity * 100).toFixed(1) + "%" : "n/a"
    }`,
  );
  for (const r of rows) {
    console.log(`  ${r.name}: ${r.status}${r.drift !== null ? ` drift=${r.drift}` : ""}`);
  }
}

try {
  main();
} catch (err) {
  console.error(`Design parity comparison error (non-blocking): ${err.message}`);
  // Non-blocking: never fail the web-ui check on design parity.
  process.exit(0);
}
