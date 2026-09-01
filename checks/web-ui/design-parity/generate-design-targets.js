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
 * locally (no network). The manifest drives the design's real navigation and
 * identifies the expected rendered surface before a screenshot is accepted.
 */
const fs = require("fs");
const path = require("path");

function validateManifest(manifest) {
  const errors = [];
  const names = new Set();
  if (!Array.isArray(manifest.views) || manifest.views.length === 0) {
    errors.push("views must be a non-empty array");
  } else {
    for (const [index, view] of manifest.views.entries()) {
      const label = view?.name || `views[${index}]`;
      if (typeof view?.name !== "string" || !view.name) errors.push(`${label}.name must be a non-empty string`);
      if (names.has(view?.name)) errors.push(`views contains duplicate name ${view.name}`);
      names.add(view?.name);
      if (typeof view?.route !== "string") errors.push(`${label}.route must be a string`);
      if (view?.dioxusRoute !== undefined && typeof view.dioxusRoute !== "string") {
        errors.push(`${label}.dioxusRoute must be a string`);
      }
      if (!view?.designMarker || typeof view.designMarker.selector !== "string") {
        errors.push(`${label}.designMarker.selector must be a string`);
      }
      if (!view?.dioxusMarker || typeof view.dioxusMarker.selector !== "string") {
        errors.push(`${label}.dioxusMarker.selector must be a string`);
      }
      for (const actionField of ["designActions", "dioxusActions"]) {
        for (const [actionIndex, action] of (view?.[actionField] || []).entries()) {
          if (action?.type !== "click" || typeof action.selector !== "string") {
            errors.push(`${label}.${actionField}[${actionIndex}] must be a click action with a selector`);
          }
          if (action?.force !== undefined && typeof action.force !== "boolean") {
            errors.push(`${label}.${actionField}[${actionIndex}].force must be a boolean`);
          }
        }
      }
    }
  }
  if (errors.length) throw new Error(`invalid design-parity manifest: ${errors.join("; ")}`);
}

function actionLocator(page, action) {
  let locator = page.locator(action.selector);
  if (action.text) locator = locator.filter({ hasText: action.text });
  return locator.nth(action.index || 0);
}

async function runActions(page, actions = []) {
  for (const action of actions) {
    const locator = actionLocator(page, action);
    await locator.waitFor({ state: "visible", timeout: action.timeout || 15000 });
    await locator.click({ force: action.force === true });
    if (action.waitFor) {
      await page.locator(action.waitFor).first().waitFor({ state: "visible", timeout: action.timeout || 15000 });
    }
  }
}

async function assertMarker(page, marker, label) {
  const locator = page.locator(marker.selector).first();
  await locator.waitFor({ state: "visible", timeout: marker.timeout || 15000 });
  if (marker.text) {
    const text = (await locator.textContent()) || "";
    if (!text.includes(marker.text)) {
      throw new Error(`${label} marker ${marker.selector} did not contain ${JSON.stringify(marker.text)}`);
    }
  }
  if (marker.attribute) {
    const value = await locator.getAttribute(marker.attribute);
    if (value !== marker.value) {
      throw new Error(
        `${label} marker ${marker.selector} had ${marker.attribute}=${JSON.stringify(value)}, expected ${JSON.stringify(marker.value)}`,
      );
    }
  }
}

async function selectTheme(page, theme) {
  let rendered = await page.locator("html").getAttribute("data-theme");
  if (rendered !== theme) {
    await page.locator('[aria-label="Toggle theme"]').click();
    await page.waitForFunction((expected) => document.documentElement.getAttribute("data-theme") === expected, theme);
    rendered = await page.locator("html").getAttribute("data-theme");
  }
  if (rendered !== theme) throw new Error(`rendered theme ${JSON.stringify(rendered)}, expected ${JSON.stringify(theme)}`);
}

async function main() {
  const { chromium } = require("playwright");
  const designDir = process.argv[2];
  const manifestPath = process.argv[3];
  const outputDir = process.argv[4] || "/tmp/design-targets";

  if (!designDir || !manifestPath) {
    console.error("usage: node generate-design-targets.js <designDir> <manifest> <outputDir>");
    process.exit(2);
  }

  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  validateManifest(manifest);
  const themes = manifest.settings.themes || ["dark", "light"];
  const viewport = manifest.settings.viewport || { width: 1920, height: 1080 };

  fs.mkdirSync(outputDir, { recursive: true });

  const htmlPath = path.join(designDir, "crystal-forge.html");
  if (!fs.existsSync(htmlPath)) {
    console.error(`FATAL: design example not found at ${htmlPath}`);
    process.exit(1);
  }
  const baseFileUrl = "file://" + htmlPath;

  // --allow-file-access-from-files: lets Chromium load file:// subresources.
  // --no-sandbox + --disable-dev-shm-usage: required inside Nix sandbox /
  // containers where /dev/shm is absent and the kernel sandbox is unavailable.
  const browser = await chromium.launch({
    args: [
      "--allow-file-access-from-files",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--disable-setuid-sandbox",
    ],
  });
  const context = await browser.newContext({
    viewport,
    timezoneId: "UTC",
    locale: "en-US",
  });

  const results = [];
  for (const view of manifest.views) {
    for (const theme of themes) {
      const name = `${view.name}--${theme}`;
      const page = await context.newPage();
      let ok = true;
      let error = null;
      try {
        await page.goto(baseFileUrl, { waitUntil: "networkidle", timeout: 45000 });
        // Babel-standalone compiles the JSX at runtime; wait for the app shell
        // and the routed content to render.
        await page.waitForSelector(".app .content", { timeout: 30000 });
        await selectTheme(page, theme);
        await runActions(page, view.designActions);
        await assertMarker(page, view.designMarker, `${name} design target`);
        // Settle animations / async coach suppression.
        await page.waitForTimeout(800);
        await page.screenshot({ path: path.join(outputDir, `${name}.design.png`) });
        console.log(`  OK design target: ${name}`);
      } catch (err) {
        ok = false;
        error = err.message;
        console.error(`  FAIL design target: ${name} - ${error}`);
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
  if (okCount !== results.length) process.exitCode = 1;
}

if (require.main === module) {
  main().catch((err) => {
    console.error(`Fatal error: ${err.message}`);
    console.error(err.stack);
    process.exit(1);
  });
}

module.exports = { assertMarker, runActions, validateManifest };
