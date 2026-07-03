/**
 * Design-parity target generator.
 *
 * Renders the Crystal Forge design example (docs/design/CrystalForge, an
 * offline/vendored copy) headlessly for each view + theme in
 * design-parity/manifest.json, backed by the shared golden fixture
 * (docs/design/CrystalForge/fixtures/crystal-forge.fixtures.json), and saves a
 * "design target" screenshot per view/theme.
 *
 * The real Dioxus screenshots for the same views are captured by the web-ui
 * integration test; compare-design-parity.js then scores the two sides.
 *
 * Usage:
 *   node generate-design-targets.js <designDir> <manifest> <outputDir>
 *
 *   <designDir>   Directory containing the offline design example
 *                 (crystal-forge.html + vendored react/babel + assets).
 *   <manifest>    Path to design-parity/manifest.json.
 *   <outputDir>   Where <view>--<theme>.design.png files are written.
 *
 * The design example must be reachable via file:// with all scripts vendored
 * locally (no network). We drive the view/theme through the query string hook
 * added to app.jsx (?view=&theme=).
 */
const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

async function main() {
  const designDir = process.argv[2];
  const manifestPath = process.argv[3];
  const outputDir = process.argv[4] || "/tmp/design-targets";

  if (!designDir || !manifestPath) {
    console.error("usage: node generate-design-targets.js <designDir> <manifest> <outputDir>");
    process.exit(2);
  }

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const themes = manifest.settings.themes || ["dark", "light"];
  const viewport = manifest.settings.viewport || { width: 1920, height: 1080 };

  fs.mkdirSync(outputDir, { recursive: true });

  const htmlPath = path.join(designDir, "crystal-forge.html");
  if (!fs.existsSync(htmlPath)) {
    console.error(`FATAL: design example not found at ${htmlPath}`);
    process.exit(1);
  }
  const baseFileUrl = "file://" + htmlPath;

  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport,
    timezoneId: "UTC",
    locale: "en-US",
  });

  const results = [];
  for (const view of manifest.views) {
    for (const theme of themes) {
      const name = `${view.name}--${theme}`;
      const url = `${baseFileUrl}?view=${encodeURIComponent(view.designView)}&theme=${theme}`;
      const page = await context.newPage();
      let ok = true;
      let error = null;
      try {
        await page.goto(url, { waitUntil: "networkidle", timeout: 45000 });
        // Babel-standalone compiles the JSX at runtime; wait for the app shell
        // and the routed content to render.
        await page.waitForSelector(".app .content", { timeout: 30000 });
        await page.waitForFunction(
          () => document.documentElement.getAttribute("data-theme"),
          { timeout: 10000 },
        );
        // Settle animations / async coach suppression.
        await page.waitForTimeout(800);
        await page.screenshot({ path: path.join(outputDir, `${name}.design.png`) });
        console.log(`  OK design target: ${name}`);
      } catch (err) {
        ok = false;
        error = err.message;
        console.error(`  FAIL design target: ${name} - ${error}`);
        try {
          await page.screenshot({ path: path.join(outputDir, `${name}.design.png`) });
        } catch (_) {}
      }
      results.push({ name, view: view.name, theme, ok, error });
      await page.close();
    }
  }

  await context.close();
  await browser.close();

  fs.writeFileSync(
    path.join(outputDir, "design-targets.json"),
    JSON.stringify({ results }, null, 2),
  );

  const okCount = results.filter((r) => r.ok).length;
  console.log(`Design targets: ${okCount}/${results.length} rendered`);
}

main().catch((err) => {
  console.error(`Fatal error: ${err.message}`);
  console.error(err.stack);
  process.exit(1);
});
