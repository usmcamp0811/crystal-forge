/**
 * Screenshot runner for Crystal Forge Web UI tests.
 *
 * Usage: node screenshot-runner.js <baseUrl> <outputDir>
 *
 * Reads route definitions from routes.js and captures screenshots
 * with optional assertions.
 */
const { chromium } = require("playwright");
const routes = require("./routes");
const fs = require("fs");

const baseUrl = process.argv[2] || "http://127.0.0.1:8080";
const outputDir = process.argv[3] || "/tmp/screenshots";

/**
 * Assert that a selector is visible on the page.
 */
async function assertVisible(page, selector, timeout = 5000) {
  try {
    await page.locator(selector).first().waitFor({ state: "visible", timeout });
    return true;
  } catch (err) {
    throw new Error(`Expected "${selector}" to be visible`);
  }
}

/**
 * Assert that a selector is NOT visible on the page.
 */
async function assertNotVisible(page, selector) {
  const visible = await page
    .locator(selector)
    .first()
    .isVisible()
    .catch(() => false);
  if (visible) {
    throw new Error(`Expected "${selector}" to NOT be visible`);
  }
  return true;
}

(async () => {
  console.log(`Starting screenshot runner`);
  console.log(`  Base URL: ${baseUrl}`);
  console.log(`  Output: ${outputDir}`);
  console.log(`  Routes: ${routes.length}`);
  console.log("");

  const browser = await chromium.launch();
  const results = [];

  for (const route of routes) {
    console.log(`Processing: ${route.name}`);
    let page;
    let ok = true;
    let error = null;

    try {
      page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });

      // Build URL with optional auth param
      const url = route.auth
        ? `${baseUrl}${route.path}?ui_check_auth=1`
        : `${baseUrl}${route.path}`;

      await page.goto(url, { waitUntil: "networkidle" });

      // Wait for WASM to hydrate
      await page.waitForTimeout(1000);

      // Run optional setup function (click buttons, open modals, etc.)
      if (route.setup) {
        await route.setup(page);
      }

      // Run mustShow assertions
      if (route.mustShow) {
        for (const selector of route.mustShow) {
          await assertVisible(page, selector);
        }
      }

      // Run mustNotShow assertions
      if (route.mustNotShow) {
        for (const selector of route.mustNotShow) {
          await assertNotVisible(page, selector);
        }
      }

      // Take screenshot
      const outputPath = `${outputDir}/${route.name}.png`;
      await page.screenshot({ path: outputPath });

      const stats = fs.statSync(outputPath);
      console.log(`  OK: ${route.name}.png (${stats.size} bytes)`);
    } catch (err) {
      ok = false;
      error = err.message;
      console.error(`  FAIL: ${route.name} - ${error}`);

      // Still try to take screenshot for debugging
      if (page) {
        try {
          const outputPath = `${outputDir}/${route.name}.png`;
          await page.screenshot({ path: outputPath });
        } catch (_) {
          // Ignore screenshot errors on failure
        }
      }
    }

    if (page) {
      await page.close();
    }

    results.push({
      name: route.name,
      path: route.path,
      ok,
      error,
    });
  }

  await browser.close();

  // Write results JSON
  fs.writeFileSync(`${outputDir}/results.json`, JSON.stringify(results, null, 2));

  // Summary
  const okCount = results.filter((r) => r.ok).length;
  const failCount = results.filter((r) => !r.ok).length;

  console.log("");
  console.log("=== Summary ===");
  console.log(`  Passed: ${okCount}/${results.length}`);
  console.log(`  Failed: ${failCount}/${results.length}`);

  if (failCount > 0) {
    console.log("");
    console.log("Failed tests:");
    for (const r of results.filter((r) => !r.ok)) {
      console.log(`  - ${r.name}: ${r.error}`);
    }
    process.exit(1);
  }
})().catch((err) => {
  console.error(`Fatal error: ${err.message}`);
  process.exit(1);
});
