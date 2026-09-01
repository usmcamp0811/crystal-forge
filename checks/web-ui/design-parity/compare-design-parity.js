/**
 * Compares authoritative React targets with matching Dioxus captures.
 *
 * The RMSE metric is advisory. Retained montages are the review evidence.
 */
const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

function run(program, args, tolerateDifference = false) {
  try {
    return execFileSync(program, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  } catch (error) {
    const output = `${error.stdout || ""}${error.stderr || ""}`.trim();
    if (tolerateDifference && error.status === 1) return output;
    throw new Error(`${program} failed: ${output || error.message}`);
  }
}

function dimensions(file) {
  const output = run("identify", ["-format", "%wx%h", file]);
  const match = output.match(/^(\d+)x(\d+)$/);
  return match ? { width: Number(match[1]), height: Number(match[2]) } : null;
}

function normalize(src, dst, width) {
  run("convert", [src, "-resize", `${width}x`, "-background", "white", "-flatten", dst]);
}

function crop(src, dst, surface) {
  const x = Math.max(0, Math.floor(surface.x));
  const y = Math.max(0, Math.floor(surface.y));
  const width = Math.max(1, Math.floor(surface.width));
  const height = Math.max(1, Math.floor(surface.height));
  run("convert", [src, "-crop", `${width}x${height}+${x}+${y}`, "+repage", dst]);
}

function alignHeights(src, dst, width, height) {
  run("convert", [src, "-gravity", "north", "-crop", `${width}x${height}+0+0`, "+repage", dst]);
}

function rmse(a, b) {
  const output = run("compare", ["-metric", "RMSE", a, b, "null:"], true);
  const match = output.match(/\(([0-9.eE+-]+)\)/);
  const score = match ? Number.parseFloat(match[1]) : NaN;
  return Number.isFinite(score) ? score : null;
}

function sameViewport(actual, expected) {
  return actual?.width === expected.width && actual?.height === expected.height;
}

function main() {
  const manifestPath = process.argv[2];
  const designDir = process.argv[3];
  const dioxusDir = process.argv[4];
  const outDir = process.argv[5] || "/tmp/design-parity";
  if (!manifestPath || !designDir || !dioxusDir) {
    throw new Error("usage: node compare-design-parity.js <manifest> <designDir> <dioxusDir> <outDir>");
  }

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const themes = manifest.settings.themes || ["dark", "light"];
  const width = manifest.settings.compare?.resizeWidth || 960;
  const captures = [
    ...(process.env.CF_TASK440_TARGETS ? [] : manifest.views.map((view) => ({ ...view, group: "primary", viewport: manifest.settings.viewport }))),
    ...(manifest.targets?.task440 || [])
      .filter((target) => !process.env.CF_TASK440_TARGETS || process.env.CF_TASK440_TARGETS.split(",").includes(target.name))
      .map((target) => ({ ...target, group: "task440" })),
  ];
  const expectedTask440 = captures.filter((capture) => capture.group === "task440").length * themes.length;
  const targetResultsPath = path.join(designDir, "design-targets.json");
  const targetResults = fs.existsSync(targetResultsPath)
    ? new Map(JSON.parse(fs.readFileSync(targetResultsPath, "utf8")).results.map((result) => [result.name, result]))
    : new Map();
  const dioxusContractsPath = path.join(dioxusDir, "task440-semantic-contracts.json");
  const dioxusContracts = fs.existsSync(dioxusContractsPath)
    ? new Map(JSON.parse(fs.readFileSync(dioxusContractsPath, "utf8")).results.map((result) => [result.name, result]))
    : new Map();
  const normDir = path.join(outDir, "normalized");
  const montageDir = path.join(outDir, "montages");
  const diffDir = path.join(outDir, "diffs");
  fs.mkdirSync(normDir, { recursive: true });
  fs.mkdirSync(montageDir, { recursive: true });
  fs.mkdirSync(diffDir, { recursive: true });

  const rows = [];
  for (const capture of captures) {
    for (const theme of themes) {
      const name = `${capture.name}--${theme}`;
      const designPng = path.join(designDir, `${name}.design.png`);
      const dioxusPng = path.join(dioxusDir, `${name}.dioxus.png`);
      const targetResult = targetResults.get(name);
      const row = {
        name,
        group: capture.group,
        view: capture.name,
        theme,
        viewport: capture.viewport,
        dioxusStep: capture.dioxusStep || null,
        route: capture.route || null,
        designRef: capture.designRef || null,
        hasDesign: fs.existsSync(designPng),
        hasDioxus: fs.existsSync(dioxusPng),
        designGenerationError: targetResult && !targetResult.ok ? targetResult.error : null,
        semanticContract: capture.group === "task440" ? {
          design: targetResult?.semanticContract || { name: capture.name, ok: false, error: "missing design semantic result" },
          dioxus: dioxusContracts.get(name) || { name: capture.name, ok: false, error: "missing Dioxus semantic result" },
        } : null,
        designDimensions: null,
        dioxusDimensions: null,
        drift: null,
        similarity: null,
        status: "missing",
      };

      if (row.designGenerationError) {
        row.status = "design-target-failed";
      } else if (row.semanticContract && (!row.semanticContract.design.ok || !row.semanticContract.dioxus.ok)) {
        row.status = "semantic-contract-failed";
      } else if (row.hasDesign && row.hasDioxus) {
        try {
          row.designDimensions = dimensions(designPng);
          row.dioxusDimensions = dimensions(dioxusPng);
          if (!sameViewport(row.designDimensions, capture.viewport) || !sameViewport(row.dioxusDimensions, capture.viewport)) {
            row.status = "viewport-mismatch";
          } else {
            const designSource = path.join(normDir, `${name}.design-source.png`);
            const dioxusSource = path.join(normDir, `${name}.dioxus-source.png`);
            if (capture.group === "task440") {
              crop(designPng, designSource, targetResult.contentSurface);
              crop(dioxusPng, dioxusSource, dioxusContracts.get(name).contentSurface);
            } else {
              run("convert", [designPng, designSource]);
              run("convert", [dioxusPng, dioxusSource]);
            }
            const resizedDesign = path.join(normDir, `${name}.design-resized.png`);
            const resizedDioxus = path.join(normDir, `${name}.dioxus-resized.png`);
            normalize(designSource, resizedDesign, width);
            normalize(dioxusSource, resizedDioxus, width);
            const designHeight = dimensions(resizedDesign).height;
            const dioxusHeight = dimensions(resizedDioxus).height;
            const comparedHeight = Math.min(designHeight, dioxusHeight);
            const normalizedDesign = path.join(normDir, `${name}.design.png`);
            const normalizedDioxus = path.join(normDir, `${name}.dioxus.png`);
            alignHeights(resizedDesign, normalizedDesign, width, comparedHeight);
            alignHeights(resizedDioxus, normalizedDioxus, width, comparedHeight);
            const score = rmse(normalizedDesign, normalizedDioxus);
            if (score === null) {
              row.status = "compare-error";
            } else {
              row.drift = Number(score.toFixed(4));
              row.similarity = Number((1 - score).toFixed(4));
              row.status = "compared";
              row.comparedSurface = { width, height: comparedHeight };
              row.diffImage = `diffs/${name}.difference.png`;
              run("convert", [normalizedDesign, normalizedDioxus, "+append", path.join(montageDir, `${name}.montage.png`)]);
              run("convert", [normalizedDesign, normalizedDioxus, "-compose", "difference", "-composite", path.join(diffDir, `${name}.difference.png`)]);
            }
          }
        } catch (error) {
          row.status = "compare-error";
          row.error = error.message;
        }
      } else if (!row.hasDesign && row.hasDioxus) {
        row.status = "missing-design";
      } else if (row.hasDesign && !row.hasDioxus) {
        row.status = "missing-dioxus";
      }
      rows.push(row);
    }
  }

  const montageFiles = fs.readdirSync(montageDir).filter((file) => file.endsWith(".montage.png")).sort();
  const gridPath = path.join(outDir, "design-parity-matrix.png");
  if (montageFiles.length) {
    try {
      run("convert", [
        ...montageFiles.map((file) => path.join(montageDir, file)),
        "-append", gridPath,
      ]);
    } catch (error) {
      console.error(`  Matrix creation failed: ${error.message}`);
    }
  }

  const compared = rows.filter((row) => row.status === "compared");
  const task440Compared = compared.filter((row) => row.group === "task440");
  const avgDrift = compared.length
    ? Number((compared.reduce((total, row) => total + row.drift, 0) / compared.length).toFixed(4))
    : null;
  const avgSimilarity = avgDrift === null ? null : Number((1 - avgDrift).toFixed(4));
  const worst = compared.slice().sort((a, b) => b.drift - a.drift).slice(0, 5).map((row) => ({ name: row.name, drift: row.drift }));
  const report = {
    version: 4,
    blocking: "semantic-contract-and-comparison-status",
    advisory: true,
    reviewRequirement: "Similarity is not a parity verdict. Inspect each content-surface montage and absolute-difference image.",
    gridImage: fs.existsSync(gridPath) ? "design-parity-matrix.png" : null,
    themes,
    counts: {
      captures: captures.length,
      compared: compared.length,
      task440Targets: captures.filter((capture) => capture.group === "task440").length,
      task440Compared: task440Compared.length,
      missing: rows.filter((row) => row.status.startsWith("missing")).length,
      errors: rows.filter((row) => row.status.includes("error") || row.status.includes("failed") || row.status === "viewport-mismatch").length,
    },
    avgDrift,
    avgSimilarity,
    worst,
    rows,
  };
  fs.writeFileSync(path.join(outDir, "design-drift-report.json"), JSON.stringify(report, null, 2));

  const markdown = [
    "## Design Parity",
    "Semantic contracts and successful content-surface comparison are blocking. The RMSE score remains advisory and is not a visual-parity verdict. Inspect every montage and absolute-difference image.",
    avgSimilarity === null
      ? "**Overall design similarity:** no comparable captures were produced."
      : `**Overall design similarity:** ${(avgSimilarity * 100).toFixed(1)}% (avg drift ${avgDrift}) across ${compared.length} captures.`,
    `**TASK-440 actual comparisons:** ${task440Compared.length}/${expectedTask440}.`,
  ];
  const notCompared = rows.filter((row) => row.status !== "compared");
  if (notCompared.length) markdown.push(`**Not compared:** ${notCompared.map((row) => `\`${row.name}\` (${row.status})`).join(", ")}`);
  markdown.push(
    "| target | group | theme | viewport | drift | similarity | status |\n" +
      "| --- | --- | --- | --- | --- | --- | --- |\n" +
      rows.map((row) => `| ${row.view} | ${row.group} | ${row.theme} | ${row.viewport.width}x${row.viewport.height} | ${row.drift ?? "-"} | ${row.similarity === null ? "-" : `${(row.similarity * 100).toFixed(1)}%`} | ${row.status} |`).join("\n"),
  );
  fs.writeFileSync(path.join(outDir, "design-drift-summary.md"), `${markdown.join("\n\n")}\n`);

  console.log("=== Design Parity ===");
  console.log(`  Compared: ${compared.length}/${rows.length}; TASK-440: ${task440Compared.length}/${expectedTask440}`);
  for (const row of rows) console.log(`  ${row.name}: ${row.status}${row.drift === null ? "" : ` drift=${row.drift}`}`);
  const failedTask440 = rows.filter((row) => row.group === "task440" && row.status !== "compared");
  if (task440Compared.length !== expectedTask440 || failedTask440.length) {
    throw new Error(`TASK-440 comparison contract failed: ${task440Compared.length}/${expectedTask440} compared; failures: ${failedTask440.map((row) => `${row.name} (${row.status})`).join(", ")}`);
  }
}

try {
  main();
} catch (error) {
  console.error(`Design parity comparison error: ${error.message}`);
  process.exit(1);
}
