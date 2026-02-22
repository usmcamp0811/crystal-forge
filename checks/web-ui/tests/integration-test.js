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

// Screenshot steps - executed in order
const steps = [
  // ============================================================
  // AUTH FLOW
  // ============================================================
  {
    name: "01-login-page",
    description: "Initial login page (first visit)",
    action: async (page) => {
      await page.goto(`${baseUrl}/login`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
      // Should either show login or redirect to register
    },
  },
  {
    name: "02-registration",
    description: "Registration page with form filled",
    action: async (page) => {
      await page.goto(`${baseUrl}/register`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);

      // Fill out registration form
      await page.fill('input[placeholder="admin"]', TEST_USER.username);
      await page.fill('input[placeholder="admin@example.com"]', TEST_USER.email);
      await page.fill('input[placeholder="Optional"]', TEST_USER.firstName, {
        strict: false,
      });
      await page.fill('input[placeholder="Minimum 8 characters"]', TEST_USER.password);
      await page.fill('input[placeholder="Re-enter password"]', TEST_USER.password);

      await page.waitForTimeout(500);
    },
  },
  {
    name: "03-registration-submit",
    description: "After clicking register",
    action: async (page) => {
      // Click submit button
      await page.click('button:has-text("Create Administrator Account")');
      await page.waitForTimeout(2000);
      // Should redirect to login after successful registration
    },
  },
  {
    name: "04-post-register-login",
    description: "Login page after registration",
    action: async (page) => {
      await page.goto(`${baseUrl}/login`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);

      // Fill login form
      await page.fill('input[placeholder="Enter your username"]', TEST_USER.username);
      await page.fill('input[placeholder="Enter your password"]', TEST_USER.password);
      await page.waitForTimeout(500);
    },
  },
  {
    name: "05-login-submit",
    description: "After clicking sign in",
    action: async (page) => {
      // Click sign in
      await page.click('button:has-text("Sign In")');
      await page.waitForTimeout(2000);
      // Should redirect to dashboard
    },
  },

  // ============================================================
  // AUTHENTICATED ROUTES
  // ============================================================
  {
    name: "06-dashboard",
    description: "Dashboard after login",
    action: async (page) => {
      await page.goto(`${baseUrl}/`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
    },
  },
  {
    name: "07-user-menu",
    description: "User dropdown menu",
    action: async (page) => {
      await page.goto(`${baseUrl}/`, { waitUntil: "networkidle" });
      await page.waitForTimeout(500);
      // Click user menu
      const userMenu = page.locator("[data-testid='user-menu-button']");
      if (await userMenu.isVisible()) {
        await userMenu.click();
        await page.waitForTimeout(300);
      }
    },
  },
  {
    name: "08-systems",
    description: "Systems list",
    action: async (page) => {
      await page.goto(`${baseUrl}/systems`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
    },
  },
  {
    name: "09-flakes",
    description: "Flakes registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/flakes`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
    },
  },
  {
    name: "10-environments",
    description: "Environments registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/environments`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
    },
  },
  {
    name: "11-builds",
    description: "Builds page",
    action: async (page) => {
      await page.goto(`${baseUrl}/builds`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
    },
  },
  {
    name: "12-cves",
    description: "CVE dashboard",
    action: async (page) => {
      await page.goto(`${baseUrl}/cves`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
    },
  },
  {
    name: "13-style-guide",
    description: "Style guide",
    action: async (page) => {
      await page.goto(`${baseUrl}/style-guide`, { waitUntil: "networkidle" });
      await page.waitForTimeout(1000);
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
    process.exit(1);
  }
})().catch((err) => {
  console.error(`Fatal error: ${err.message}`);
  process.exit(1);
});
