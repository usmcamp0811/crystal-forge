const { chromium } = require("playwright");
const routes = require("./routes");
const fs = require("fs");

const baseUrl = process.argv[2];
const outputDir = process.argv[3];

async function assertVisible(page, selector) {
  await page
    .locator(selector)
    .first()
    .waitFor({ state: "visible", timeout: 5000 });
}

async function assertNotVisible(page, selector) {
  const visible = await page
    .locator(selector)
    .first()
    .isVisible()
    .catch(() => false);
  if (visible) {
    throw new Error(`Unexpected visibility: ${selector}`);
  }
}

(async () => {
  const browser = await chromium.launch();
  const results = [];

  for (const route of routes) {
    const page = await browser.newPage({
      viewport: { width: 1920, height: 1080 },
    });

    const url = route.auth
      ? `${baseUrl}${route.path}?ui_check_auth=1`
      : `${baseUrl}${route.path}`;

    try {
      await page.goto(url, { waitUntil: "networkidle" });
      await page.waitForTimeout(800);

      if (route.mustShow) {
        for (const selector of route.mustShow) {
          await assertVisible(page, selector);
        }
      }

      if (route.mustNotShow) {
        for (const selector of route.mustNotShow) {
          await assertNotVisible(page, selector);
        }
      }

      const outputPath = `${outputDir}/${route.name}.png`;
      await page.screenshot({ path: outputPath });

      results.push({ name: route.name, ok: true });
    } catch (err) {
      results.push({ name: route.name, ok: false, error: err.message });
    }

    await page.close();
  }

  await browser.close();
  fs.writeFileSync(
    `${outputDir}/results.json`,
    JSON.stringify(results, null, 2),
  );

  if (results.some((r) => !r.ok)) process.exit(1);
})();
