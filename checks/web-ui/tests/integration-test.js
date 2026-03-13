/**
 * Crystal Forge Web UI Integration Test
 *
 * This test:
 * 1. Navigates to the login page (should redirect to register on first run)
 * 2. Registers an admin user
 * 3. Logs in with that user
 * 4. Takes screenshots of all major authenticated routes
 *
 * Usage: node integration-test.js <baseUrl> <outputDir>
 */
const { chromium } = require("playwright");
const fs = require("fs");

const baseUrl = process.argv[2] || "http://127.0.0.1:3000";
const outputDir = process.argv[3] || "/tmp/screenshots";

// Test user credentials
const TEST_USER = {
  username: "admin",
  email: "admin@example.com",
  password: "testpassword123",
  firstName: "Test",
  lastName: "Admin",
};

// Timeout for page loads (don't use networkidle as it can hang)
const LOAD_TIMEOUT = 10000;

// Screenshot steps - executed in order
const steps = [
  // ============================================================
  // AUTH FLOW
  // ============================================================
  {
    name: "01-login-page",
    description: "Initial login page (first visit)",
    action: async (page) => {
      await page.goto(`${baseUrl}/login`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000); // Wait for WASM hydration
    },
  },
  {
    name: "02-registration",
    description: "Registration page with form filled",
    action: async (page) => {
      await page.goto(`${baseUrl}/register`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000); // Wait for WASM hydration

      // Fill out registration form - use more robust selectors
      await page.locator('input[type="text"]').first().fill(TEST_USER.username);
      await page.locator('input[type="email"]').fill(TEST_USER.email);
      await page.locator('input[type="password"]').first().fill(TEST_USER.password);
      await page.locator('input[type="password"]').last().fill(TEST_USER.password);

      await page.waitForTimeout(500);
    },
  },
  {
    name: "03-registration-submit",
    description: "After clicking register",
    action: async (page) => {
      // Click submit button
      const submitBtn = page.locator('button[type="submit"]');
      await submitBtn.click();
      await page.waitForTimeout(3000); // Wait for registration + redirect
    },
  },
  {
    name: "04-post-register-login",
    description: "Login page after registration",
    action: async (page) => {
      await page.goto(`${baseUrl}/login`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      // Fill login form
      await page.locator('input[type="text"]').fill(TEST_USER.username);
      await page.locator('input[type="password"]').fill(TEST_USER.password);
      await page.waitForTimeout(500);
    },
  },
  {
    name: "05-login-submit",
    description: "After clicking sign in",
    action: async (page) => {
      // Click sign in
      const submitBtn = page.locator('button[type="submit"]');
      await submitBtn.click();
      await page.waitForTimeout(3000); // Wait for login + redirect
    },
  },

  // ============================================================
  // AUTHENTICATED ROUTES
  // ============================================================
  {
    name: "06-dashboard",
    description: "Dashboard after login",
    action: async (page) => {
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "07-user-menu",
    description: "User dropdown menu",
    action: async (page) => {
      // Click user menu if visible
      const userMenu = page.locator("[data-testid='user-menu-button']");
      if (await userMenu.isVisible({ timeout: 3000 }).catch(() => false)) {
        await userMenu.click();
        await page.waitForTimeout(500);
      }
    },
  },
  {
    name: "08-systems",
    description: "Systems list",
    action: async (page) => {
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "09-flakes",
    description: "Flakes registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "10-environments",
    description: "Environments registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/environments`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "11-builds",
    description: "Builds page",
    action: async (page) => {
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const queueCards = page.locator("[data-testid='build-queue-card']");
      const queueCardCount = await queueCards.count();
      if (queueCardCount > 0) {
        const overflowingCards = await queueCards.evaluateAll((cards) =>
          cards.filter((card) => card.scrollWidth > card.clientWidth + 1).length,
        );
        if (overflowingCards > 0) {
          throw new Error(`Build queue has ${overflowingCards} overflowing cards`);
        }
      }
    },
  },
  {
    name: "11b-builds-queue-card-focus",
    description: "Build queue card layout focus",
    action: async (page) => {
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const firstQueueCard = page.locator("[data-testid='build-queue-card']").first();
      if (await firstQueueCard.isVisible({ timeout: 2000 }).catch(() => false)) {
        await firstQueueCard.click();
        await page.waitForTimeout(700);
      }
    },
  },
  {
    name: "12-cves",
    description: "CVE dashboard",
    action: async (page) => {
      await page.goto(`${baseUrl}/cves`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "13-style-guide",
    description: "Style guide",
    action: async (page) => {
      await page.goto(`${baseUrl}/style-guide`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "14-policies",
    description: "Policies view",
    action: async (page) => {
      await page.goto(`${baseUrl}/deployment-policies`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
      await page.locator("main h1:has-text('Deployment Policies')").first().waitFor({ timeout: 5000 });
    },
  },
  {
    name: "15-policies-new-modal-basic",
    description: "Policies new modal in basic mode",
    action: async (page) => {
      await page.goto(`${baseUrl}/deployment-policies`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
      const newPolicyBtn = page.locator("button:has-text('New Policy')").first();
      await newPolicyBtn.waitFor({ timeout: 5000 });
      await newPolicyBtn.click();
      await page.waitForTimeout(1200);
      await page.getByRole("heading", { name: "Create Policy" }).waitFor({ timeout: 5000 });
    },
  },
  {
    name: "16-policies-new-modal-advanced",
    description: "Policies new modal in advanced mode",
    action: async (page) => {
      await page.goto(`${baseUrl}/deployment-policies`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
      const newPolicyBtn = page.locator("button:has-text('New Policy')").first();
      await newPolicyBtn.waitFor({ timeout: 5000 });
      await newPolicyBtn.click();
      await page.waitForTimeout(700);
      const advancedBtn = page.getByRole("button", { name: "Advanced" });
      await advancedBtn.waitFor({ timeout: 5000 });
      await advancedBtn.click();
      await page.waitForTimeout(1200);
      await page.getByText("Policy Definition").first().waitFor({ timeout: 5000 });
    },
  },
  {
    name: "17-caches",
    description: "Cache management view",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
    },
  },
  {
    name: "18-caches-modal-nix",
    description: "Add cache modal with Nix type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("Nix");
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "19-caches-modal-http",
    description: "Add cache modal with Http type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("Http");
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "20-caches-modal-s3",
    description: "Add cache modal with S3 type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("S3");
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "21-caches-modal-attic",
    description: "Add cache modal with Attic type selected",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);

      const addBtn = page.locator("button:has-text('Add Destination')").first();
      await addBtn.waitFor({ timeout: 5000 });
      await addBtn.click();
      await page.getByRole("heading", { name: "Add Cache Destination" }).waitFor({ timeout: 5000 });

      const dialog = page.locator("[role='dialog']").first();
      await dialog.locator("select").first().selectOption("Attic");
      await page.waitForTimeout(1200);
    },
  },
];

(async () => {
  console.log("Starting Crystal Forge Web UI Integration Test");
  console.log(`  Base URL: ${baseUrl}`);
  console.log(`  Output: ${outputDir}`);
  console.log(`  Steps: ${steps.length}`);
  console.log("");

  const browser = await chromium.launch();
  // Use a single browser context to maintain session/cookies across steps
  const context = await browser.newContext({
    viewport: { width: 1920, height: 1080 },
  });
  const page = await context.newPage();

  const results = [];

  for (const step of steps) {
    console.log(`Step: ${step.name} - ${step.description}`);
    let ok = true;
    let error = null;

    try {
      await step.action(page);

      // Take screenshot
      const outputPath = `${outputDir}/${step.name}.png`;
      await page.screenshot({ path: outputPath });

      const stats = fs.statSync(outputPath);
      console.log(`  OK: ${step.name}.png (${stats.size} bytes)`);
    } catch (err) {
      ok = false;
      error = err.message;
      console.error(`  FAIL: ${step.name} - ${error}`);

      // Try to take screenshot anyway for debugging
      try {
        const outputPath = `${outputDir}/${step.name}.png`;
        await page.screenshot({ path: outputPath });
      } catch (_) {}
    }

    results.push({
      name: step.name,
      description: step.description,
      ok,
      error,
    });
  }

  await context.close();
  await browser.close();

  // Write results
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
    console.log("Failed steps:");
    for (const r of results.filter((r) => !r.ok)) {
      console.log(`  - ${r.name}: ${r.error}`);
    }
    // Don't exit with error - let the test script analyze results
  }
})().catch((err) => {
  console.error(`Fatal error: ${err.message}`);
  console.error(err.stack);
  process.exit(1);
});
