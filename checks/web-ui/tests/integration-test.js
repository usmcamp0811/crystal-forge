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

const VIEWPORTS = {
  desktop: { width: 1440, height: 900 },
  tablet: { width: 900, height: 900 },
  narrowDesktop: { width: 560, height: 900 },
  mobile: { width: 375, height: 812 },
};

async function assertVisible(locator, message) {
  const visible = await locator.isVisible({ timeout: 5000 }).catch(() => false);
  if (!visible) {
    throw new Error(message);
  }
}

async function assertHidden(locator, message) {
  const visible = await locator.isVisible({ timeout: 1500 }).catch(() => false);
  if (visible) {
    throw new Error(message);
  }
}

function nowIso() {
  return new Date().toISOString();
}

function mockBuildsDashboardSummary() {
  const timestamp = nowIso();
  return {
    fleet_health: { healthy: 1, warning: 0, critical: 0, offline: 0 },
    deployment_status: { up_to_date: 1, behind: 0, never_deployed: 0, unknown: 0 },
    cve_summary: { critical: 0, high: 0, medium: 0, low: 0 },
    total_systems: 1,
    active_builds: 1,
    build_queue: {
      building_count: 1,
      queued_count: 1,
      timestamp,
      items: [
        {
          job_id: "11111111-1111-4111-8111-111111111111",
          system_id: "22222222-2222-4222-8222-222222222222",
          hostname: "very-long-hostname-that-should-not-overflow-build-queue-card.example.internal",
          flake_name: "critical-infra-flake-with-a-very-long-name-for-layout-testing",
          commit_hash: "a1b2c3d4e5f678901234567890abcdef12345678",
          commit_message:
            "Queued build with intentionally long metadata to validate truncation and card boundaries in the Builds queue UI",
          status: "queued",
          builder_name: "builder-primary",
          queued_at: timestamp,
          started_at: null,
          elapsed_secs: null,
          logs: null,
        },
        {
          job_id: "33333333-3333-4333-8333-333333333333",
          system_id: "44444444-4444-4444-8444-444444444444",
          hostname: "build-runner-02",
          flake_name: "platform-core",
          commit_hash: "deadbeefcafebabe1234567890abcdef12345678",
          commit_message: "Building core platform",
          status: "building",
          builder_name: "builder-secondary",
          queued_at: timestamp,
          started_at: timestamp,
          elapsed_secs: 42,
          logs: null,
        },
      ],
    },
    recent_deployments: [],
    timestamp,
  };
}

function mockBuilders() {
  const timestamp = nowIso();
  return [
    {
      id: "55555555-5555-4555-8555-555555555555",
      name: "builder-primary",
      status: "active",
      max_cpu_cores: 8,
      max_memory_mb: 16384,
      max_concurrent_jobs: 4,
      last_heartbeat_at: timestamp,
      assigned_environment_count: 1,
      active_jobs: 1,
      queued_jobs: 1,
    },
  ];
}

function mockRecentBuilds() {
  const timestamp = nowIso();
  return [
    {
      job_id: "66666666-6666-4666-8666-666666666666",
      system_id: "77777777-7777-4777-8777-777777777777",
      hostname: "history-system-1",
      flake_name: "platform-core",
      commit_hash: "abcd1234abcd1234abcd1234abcd1234abcd1234",
      commit_message: "Recent successful build",
      status: "complete",
      builder_name: "builder-primary",
      queued_at: timestamp,
      started_at: timestamp,
      elapsed_secs: 15,
      logs: null,
    },
  ];
}

async function routeBuildsData(page) {
  await page.route("**/api/v1/dashboard/summary*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockBuildsDashboardSummary()),
    });
  });

  await page.route("**/api/v1/builders*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockBuilders()),
    });
  });

  await page.route("**/api/v1/build-jobs/recent*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockRecentBuilds()),
    });
  });
}

async function unrouteBuildsData(page) {
  await page.unroute("**/api/v1/dashboard/summary*");
  await page.unroute("**/api/v1/builders*");
  await page.unroute("**/api/v1/build-jobs/recent*");
}

function mockSetupCoachProgress() {
  return {
    dismissed: false,
    agent_acknowledged: false,
    environment: { complete: false, count: 0 },
    flake: { complete: false, count: 0 },
    builder: { complete: false, count: 0 },
    cache: { complete: false, count: 0 },
    system: { complete: false, count: 0 },
    all_required_complete: false,
  };
}

async function routeSetupCoachData(page) {
  await page.route("**/api/v1/admin/setup-progress*", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(mockSetupCoachProgress()),
    });
  });
  await page.route("**/api/v1/admin/setup-wizard/dismiss*", async (route) => {
    await route.fulfill({ status: 204, body: "" });
  });
  await page.route("**/api/v1/admin/setup-wizard/agent-acknowledge*", async (route) => {
    await route.fulfill({ status: 204, body: "" });
  });
}

async function unrouteSetupCoachData(page) {
  await page.unroute("**/api/v1/admin/setup-progress*");
  await page.unroute("**/api/v1/admin/setup-wizard/dismiss*");
  await page.unroute("**/api/v1/admin/setup-wizard/agent-acknowledge*");
}

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
      await routeSetupCoachData(page);
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
      await assertVisible(
        page.locator("[data-testid='onboarding-coach-panel']"),
        "Onboarding coach panel should be visible on dashboard",
      );
    },
  },
  {
    name: "06a-onboarding-coach-dashboard",
    description: "Non-blocking onboarding coach panel on dashboard",
    action: async (page) => {
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);
      await assertVisible(
        page.locator("[data-testid='onboarding-coach-panel']"),
        "Onboarding coach panel should be visible",
      );
      await assertVisible(
        page.locator("[data-testid='onboarding-step-environment']"),
        "Environment onboarding step should be visible",
      );
    },
  },
  {
    name: "06b-onboarding-environments-callout",
    description: "Coach step -> Environments page with contextual callout",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-environment']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.getByText("You came here from the Setup Coach").first(),
        "Expected setup coach contextual callout on environments page",
      );
    },
  },
  {
    name: "06c-onboarding-flakes-callout",
    description: "Coach step -> Flakes page with contextual callout",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-flake']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.getByText("You came here from the Setup Coach").first(),
        "Expected setup coach contextual callout on flakes page",
      );
    },
  },
  {
    name: "06d-onboarding-builders-callout",
    description: "Coach step -> Builders page with contextual callout",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-builder']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.getByText("You came here from the Setup Coach").first(),
        "Expected setup coach contextual callout on builders page",
      );
    },
  },
  {
    name: "06e-onboarding-caches-callout",
    description: "Coach step -> Caches page with contextual callout",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-cache']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.getByText("You came here from the Setup Coach").first(),
        "Expected setup coach contextual callout on caches page",
      );
    },
  },
  {
    name: "06f-onboarding-systems-callout",
    description: "Coach step -> Systems page with contextual callout",
    action: async (page) => {
      await page.locator("[data-testid='onboarding-step-system']").click();
      await page.waitForTimeout(1500);
      await assertVisible(
        page.getByText("You came here from the Setup Coach").first(),
        "Expected setup coach contextual callout on systems page",
      );
    },
  },
  // ============================================================
  // RESPONSIVE SIDEBAR SCREENSHOTS
  // Each step clears localStorage first so state is deterministic.
  // ============================================================
  {
    name: "07-sidebar-desktop-expanded",
    description: "Desktop: sidebar expanded — grouped sections with labels visible",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      // Force expanded state
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "false");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Desktop: sidebar should be visible");
      await assertVisible(toggle, "Desktop: sidebar edge toggle should be visible");
      await assertHidden(
        page.locator("[data-testid='mobile-nav-toggle']"),
        "Desktop: mobile nav toggle should be hidden",
      );

      // Confirm sidebar is expanded (full width ~256px)
      const box = await sidebar.boundingBox();
      if (!box || box.width < 200) {
        throw new Error(`Desktop expanded sidebar too narrow: ${box ? box.width : "missing"}`);
      }
    },
  },
  {
    name: "08-sidebar-desktop-collapsed",
    description: "Desktop: sidebar in icons-only collapsed state with edge toggle",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "true");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Desktop collapsed: sidebar should still be visible");
      await assertVisible(toggle, "Desktop collapsed: edge toggle should be visible");

      const box = await sidebar.boundingBox();
      if (!box || box.width > 100) {
        throw new Error(`Desktop collapsed sidebar too wide: ${box ? box.width : "missing"}`);
      }
      // Screenshot taken here: collapsed icons-only state
    },
  },
  {
    name: "08b-sidebar-desktop-toggle-expand",
    description: "Desktop: sidebar expanded via toggle click — full labels and sections",
    action: async (page) => {
      // Self-contained: force collapsed, reload, then click toggle to expand
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "true");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      const collapsedBox = await sidebar.boundingBox();
      if (!collapsedBox || collapsedBox.width > 100) {
        throw new Error(`Desktop expand: expected collapsed start: ${collapsedBox ? collapsedBox.width : "missing"}`);
      }

      await toggle.click();
      await page.waitForTimeout(400);

      const expandedBox = await sidebar.boundingBox();
      if (!expandedBox || expandedBox.width < 200) {
        throw new Error(`Desktop toggle expand failed: ${expandedBox ? expandedBox.width : "missing"}`);
      }
      // Screenshot taken here: expanded state after clicking toggle
    },
  },
  {
    name: "09-sidebar-tablet-collapsed",
    description: "Tablet (900px): default icons-only state, edge toggle visible",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.tablet);
      await page.evaluate(() => {
        localStorage.removeItem("cf-sidebar-collapsed");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Tablet: sidebar should be visible");
      await assertVisible(toggle, "Tablet: edge toggle should be visible");
      await assertHidden(
        page.locator("[data-testid='mobile-nav-toggle']"),
        "Tablet: mobile nav toggle should be hidden",
      );

      const box = await sidebar.boundingBox();
      if (!box) throw new Error("Tablet: sidebar bounding box missing");
      // Screenshot taken here: collapsed icons-only at tablet width
    },
  },
  {
    name: "09b-sidebar-tablet-expanded",
    description: "Tablet (900px): sidebar expanded via toggle — section labels visible",
    action: async (page) => {
      // Self-contained: force collapsed, reload, then click toggle to expand
      await page.setViewportSize(VIEWPORTS.tablet);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "true");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const toggle = page.locator("[data-testid='sidebar-edge-toggle']");

      const collapsedBox = await sidebar.boundingBox();
      if (!collapsedBox || collapsedBox.width > 100) {
        throw new Error(`Tablet expand: expected collapsed start: ${collapsedBox ? collapsedBox.width : "missing"}`);
      }

      await toggle.click();
      await page.waitForTimeout(400);

      const expandedBox = await sidebar.boundingBox();
      if (!expandedBox || expandedBox.width < 200) {
        throw new Error(`Tablet toggle expand failed: ${expandedBox ? expandedBox.width : "missing"}`);
      }
      // Screenshot taken here: expanded labels + sections at tablet viewport
    },
  },
  {
    name: "09c-sidebar-mobile-drawer",
    description: "Mobile (375px): drawer open with grouped navigation sections",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.mobile);
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      await assertHidden(
        page.locator("[data-testid='sidebar-nav']"),
        "Mobile: sidebar should be hidden",
      );

      const mobileToggle = page.locator("[data-testid='mobile-nav-toggle']");
      await assertVisible(mobileToggle, "Mobile: hamburger toggle should be visible");
      await mobileToggle.click();
      await page.waitForTimeout(500);

      await assertVisible(
        page.locator("[data-testid='mobile-drawer']"),
        "Mobile: drawer should open after tapping hamburger",
      );
      // Screenshot taken here: mobile drawer open showing grouped sections
    },
  },
  {
    name: "09d-sidebar-narrow-collapsed",
    description: "Narrow desktop (560px): default icons-only — no hamburger, edge toggle present",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.narrowDesktop);
      await page.evaluate(() => {
        localStorage.removeItem("cf-sidebar-collapsed");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      const edgeToggle = page.locator("[data-testid='sidebar-edge-toggle']");

      await assertVisible(sidebar, "Narrow desktop: sidebar visible");
      await assertVisible(edgeToggle, "Narrow desktop: edge toggle visible");
      await assertHidden(
        page.locator("[data-testid='mobile-nav-toggle']"),
        "Narrow desktop: mobile hamburger hidden",
      );

      const box = await sidebar.boundingBox();
      if (!box || box.width > 120) {
        throw new Error(
          `Narrow desktop should default to icons-only: ${box ? box.width : "missing"}`,
        );
      }
      // Screenshot taken here: icons-only collapsed at 560px
    },
  },
  {
    name: "09e-sidebar-sections-fullwidth",
    description: "Desktop: full-width sidebar showing all section group headers",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.setItem("cf-sidebar-collapsed", "false");
      });
      await page.reload({ timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1500);

      const sidebar = page.locator("[data-testid='sidebar-nav']");
      await assertVisible(sidebar, "Sections shot: sidebar must be visible");
      const box = await sidebar.boundingBox();
      if (!box || box.width < 200) {
        throw new Error(`Sections shot: sidebar not expanded: ${box ? box.width : "missing"}`);
      }
      // Screenshot taken here: full desktop expanded sidebar, all groups visible
    },
  },
  {
    name: "10-responsive-reset-desktop",
    description: "Reset viewport and localStorage to desktop defaults for remaining screenshots",
    action: async (page) => {
      await page.setViewportSize(VIEWPORTS.desktop);
      await page.evaluate(() => {
        localStorage.removeItem("cf-sidebar-collapsed");
      });
      await page.goto(`${baseUrl}/`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(1200);
    },
  },
  {
    name: "11-user-menu",
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
    name: "12-systems",
    description: "Systems list",
    action: async (page) => {
      await page.goto(`${baseUrl}/systems`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "13-flakes",
    description: "Flakes registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/flakes`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "14-environments",
    description: "Environments registry",
    action: async (page) => {
      await page.goto(`${baseUrl}/environments`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "15-builds",
    description: "Builds page",
    action: async (page) => {
      await routeBuildsData(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const queueCards = page.locator("[data-testid='build-queue-card']");
      const queueCardCount = await queueCards.count();
      if (queueCardCount === 0) {
        throw new Error("Expected at least one build queue card in builds screenshot");
      }
      if (queueCardCount > 0) {
        const overflowingCards = await queueCards.evaluateAll((cards) =>
          cards.filter((card) => card.scrollWidth > card.clientWidth + 1).length,
        );
        if (overflowingCards > 0) {
          throw new Error(`Build queue has ${overflowingCards} overflowing cards`);
        }
      }

      await unrouteBuildsData(page);
    },
  },
  {
    name: "11b-builds-queue-card-focus",
    description: "Build queue card layout focus",
    action: async (page) => {
      await routeBuildsData(page);
      await page.goto(`${baseUrl}/builds`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);

      const firstQueueCard = page.locator("[data-testid='build-queue-card']").first();
      if (await firstQueueCard.isVisible({ timeout: 2000 }).catch(() => false)) {
        await firstQueueCard.click();
        await page.waitForTimeout(700);
      } else {
        throw new Error("Expected first build queue card to be visible for focused screenshot");
      }

      await unrouteBuildsData(page);
    },
  },
  {
    name: "16-cves",
    description: "CVE dashboard",
    action: async (page) => {
      await page.goto(`${baseUrl}/cves`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "17-style-guide",
    description: "Style guide",
    action: async (page) => {
      await page.goto(`${baseUrl}/style-guide`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2000);
    },
  },
  {
    name: "18-policies",
    description: "Policies view",
    action: async (page) => {
      await page.goto(`${baseUrl}/deployment-policies`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
      await page.locator("main h1:has-text('Deployment Policies')").first().waitFor({ timeout: 5000 });
    },
  },
  {
    name: "19-policies-new-modal-basic",
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
    name: "20-policies-new-modal-advanced",
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
    name: "21-caches",
    description: "Cache management view",
    action: async (page) => {
      await page.goto(`${baseUrl}/caches`, { timeout: LOAD_TIMEOUT });
      await page.waitForTimeout(2500);
    },
  },
  {
    name: "22-caches-modal-nix",
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
    name: "23-caches-modal-http",
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
    name: "24-caches-modal-s3",
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
    name: "25-caches-modal-attic",
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
