/**
 * Screenshot test route definitions.
 *
 * Each route object defines:
 *   - name: Screenshot filename (without .png)
 *   - path: URL path to navigate to
 *   - auth: If true, appends ?ui_check_auth=1 for mock auth
 *   - setup: Optional async function to run before screenshot (e.g., click buttons)
 *   - mustShow: Array of selectors that must be visible
 *   - mustNotShow: Array of selectors that must NOT be visible
 */
module.exports = [
  // ============================================================
  // AUTH SCREENS (unauthenticated)
  // ============================================================
  {
    name: "login",
    path: "/login",
    mustShow: ["text=Crystal Forge", "text=Sign in to continue"],
  },
  {
    name: "registration",
    path: "/register",
    mustShow: ["text=Administrator Registration", "text=First-Time Setup"],
  },

  // ============================================================
  // AUTH PROTECTION (verify redirect to login)
  // ============================================================
  {
    name: "auth-redirect-dashboard",
    path: "/",
    auth: false,
    mustShow: ["text=Sign in to continue"],
    mustNotShow: ["[data-testid='dashboard']"],
  },

  // ============================================================
  // DASHBOARD
  // ============================================================
  {
    name: "dashboard",
    path: "/",
    auth: true,
    mustShow: [
      "[data-testid='dashboard']",
      "text=Total Systems",
      "text=Healthy",
      "[data-testid='fleet-health-breakdown']",
      "[data-testid='cve-summary']",
      "[data-testid='deployment-status']",
      "[data-testid='build-summary-panel']",
      "[data-testid='build-queue']",
      "[data-testid='recent-deployments']",
      "[data-testid='flake-timeline-widget']",
    ],
  },
  {
    name: "topbar-user-dropdown",
    path: "/",
    auth: true,
    setup: async (page) => {
      await page.locator("[data-testid='user-menu-button']").click();
      await page.waitForTimeout(250);
    },
    mustShow: ["[data-testid='user-menu-dropdown']", "text=Sign Out"],
  },

  // ============================================================
  // SYSTEMS
  // ============================================================
  {
    name: "systems-table",
    path: "/systems",
    auth: true,
    mustShow: [
      "[data-testid='systems-table']",
      "text=atlas-01",
      "button:has-text('Table')",
      "button:has-text('Cards')",
    ],
  },
  {
    name: "systems-cards",
    path: "/systems",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Cards" }).click();
      await page.waitForTimeout(500);
    },
    mustShow: ["[data-testid='systems-cards']", "text=atlas-01", "text=luna-02"],
    mustNotShow: ["[data-testid='systems-table']"],
  },
  {
    name: "systems-add-modal",
    path: "/systems",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add System" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Register System", "text=Save System"],
  },
  {
    name: "systems-keypair-modal",
    path: "/systems",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add System" }).click();
      await page.waitForTimeout(250);
      await page.getByRole("button", { name: "Generate" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Generated System Key Pair", "text=Use Public Key"],
  },
  {
    name: "systems-remove-modal",
    path: "/systems",
    auth: true,
    setup: async (page) => {
      await page.locator("button:has-text('Remove')").first().click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Remove", "text=Cancel"],
  },
  {
    name: "system-detail",
    path: "/systems/00000000-0000-0000-0000-000000000001",
    auth: true,
    mustShow: [
      "[data-testid='system-detail']",
      "text=atlas-01",
      "text=Hardware",
      "text=Network",
      "text=Security",
      "text=Vulnerabilities",
      "text=Agent",
    ],
  },

  // ============================================================
  // FLAKES
  // ============================================================
  {
    name: "flakes-table",
    path: "/flakes",
    auth: true,
    setup: async (page) => {
      const tableToggle = page.getByRole("button", { name: "Table" });
      if (await tableToggle.isVisible().catch(() => false)) {
        await tableToggle.click();
        await page.waitForTimeout(200);
      }
    },
    mustShow: ["text=Flake Registry", "[data-testid='flakes-table']"],
  },
  {
    name: "flakes-cards",
    path: "/flakes",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Cards" }).click();
      await page.waitForTimeout(500);
    },
    mustShow: ["[data-testid='flakes-cards']", "text=Latest Commit"],
  },
  {
    name: "flakes-add-modal",
    path: "/flakes",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add Flake" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Register Flake", "text=Save Flake"],
  },
  {
    name: "flakes-edit-modal",
    path: "/flakes",
    auth: true,
    setup: async (page) => {
      await page.locator("button:has-text('Edit')").first().click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Edit Flake", "text=Save Changes"],
  },
  {
    name: "flakes-remove-modal",
    path: "/flakes",
    auth: true,
    setup: async (page) => {
      // Create a temp flake first, then remove it
      await page.getByRole("button", { name: "Add Flake" }).click();
      await page.getByPlaceholder("prod-core").fill("qa-temp");
      await page
        .getByPlaceholder("https://github.com/org/repo")
        .fill("https://github.com/example/qa-temp");
      await page.getByRole("button", { name: "Save Flake" }).click();
      await page.waitForTimeout(300);
      await page
        .locator("tr:has-text('qa-temp') button:has-text('Remove')")
        .first()
        .click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Remove flake", "text=Related commits are deleted by cascade"],
  },

  // ============================================================
  // ENVIRONMENTS
  // ============================================================
  {
    name: "environments-registry",
    path: "/environments",
    auth: true,
    mustShow: [
      "text=Environment Registry",
      "text=Edit Environment",
      "text=Edit Requirements",
    ],
  },
  {
    name: "environments-add-modal",
    path: "/environments",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add Environment" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Create Environment", "text=Choose Policies"],
  },
  {
    name: "environments-policy-picker-modal",
    path: "/environments",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add Environment" }).click();
      await page.waitForTimeout(200);
      await page.getByRole("button", { name: "Choose Policies" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Choose Required Policies", "text=Apply Policies"],
  },
  {
    name: "environments-edit-modal",
    path: "/environments",
    auth: true,
    setup: async (page) => {
      await page.locator("button:has-text('Edit Environment')").first().click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Edit Environment", "text=Save Changes"],
  },
  {
    name: "environments-edit-requirements-modal",
    path: "/environments",
    auth: true,
    setup: async (page) => {
      await page.locator("button:has-text('Edit Requirements')").first().click();
      await page.waitForTimeout(300);
    },
    mustShow: [
      "text=Save Requirements",
      "text=Required policies are hard requirements",
    ],
  },
  {
    name: "environments-remove-modal",
    path: "/environments",
    auth: true,
    setup: async (page) => {
      await page.locator("button:has-text('Remove')").first().click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Remove environment", "text=This deletes the environment"],
  },

  // ============================================================
  // BUILDS
  // ============================================================
  {
    name: "builds",
    path: "/builds",
    auth: true,
    mustShow: ["text=Builds"],
  },

  // ============================================================
  // CVES
  // ============================================================
  {
    name: "cves",
    path: "/cves",
    auth: true,
    mustShow: ["text=CVE"],
  },

  // ============================================================
  // STYLE GUIDE
  // ============================================================
  {
    name: "style-guide",
    path: "/style-guide",
    auth: true,
    mustShow: ["text=Style Guide"],
  },

  // ============================================================
  // 404 NOT FOUND
  // ============================================================
  {
    name: "not-found",
    path: "/not-a-real-page",
    auth: true,
    mustShow: ["text=/404|not found/i"],
  },
];
