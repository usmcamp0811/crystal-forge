/**
 * Screenshot test route definitions.
 *
 * Each route object defines:
 *   - name: Screenshot filename (without .png)
 *   - path: URL path to navigate to
 *   - auth: If true, appends ?ui_check_auth=1 for mock auth
 *   - setup: Optional async function to run before screenshot (e.g., click buttons)
 *   - mustShow: Array of selectors that must be visible (keep minimal for speed)
 *
 * TIP: Keep mustShow arrays small (1-2 selectors) - each adds timeout risk.
 */
module.exports = [
  // ============================================================
  // AUTH SCREENS
  // ============================================================
  {
    name: "login",
    path: "/login",
    mustShow: ["text=Sign in to continue"],
  },
  {
    name: "registration",
    path: "/register",
    auth: true, // Use mock to bypass setup-status check
    mustShow: ["text=First-Time Setup"],
  },
  {
    name: "auth-redirect",
    path: "/",
    auth: false,
    mustShow: ["text=Sign in to continue"],
  },

  // ============================================================
  // DASHBOARD
  // ============================================================
  {
    name: "dashboard",
    path: "/",
    auth: true,
    mustShow: ["[data-testid='dashboard']"],
  },
  {
    name: "topbar-user-dropdown",
    path: "/",
    auth: true,
    setup: async (page) => {
      await page.locator("[data-testid='user-menu-button']").click();
      await page.waitForTimeout(250);
    },
    mustShow: ["[data-testid='user-menu-dropdown']"],
  },

  // ============================================================
  // SYSTEMS
  // ============================================================
  {
    name: "systems-table",
    path: "/systems",
    auth: true,
    mustShow: ["[data-testid='systems-table']"],
  },
  {
    name: "systems-cards",
    path: "/systems",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Cards" }).click();
      await page.waitForTimeout(400);
    },
    mustShow: ["[data-testid='systems-cards']"],
  },
  {
    name: "systems-add-modal",
    path: "/systems",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add System" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Register System"],
  },
  {
    name: "system-detail",
    path: "/systems/00000000-0000-0000-0000-000000000001",
    auth: true,
    mustShow: ["[data-testid='system-detail']"],
  },

  // ============================================================
  // FLAKES
  // ============================================================
  {
    name: "flakes-table",
    path: "/flakes",
    auth: true,
    mustShow: ["[data-testid='flakes-table']"],
  },
  {
    name: "flakes-cards",
    path: "/flakes",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Cards" }).click();
      await page.waitForTimeout(400);
    },
    mustShow: ["[data-testid='flakes-cards']"],
  },
  {
    name: "flakes-add-modal",
    path: "/flakes",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add Flake" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Register Flake"],
  },

  // ============================================================
  // ENVIRONMENTS
  // ============================================================
  {
    name: "environments",
    path: "/environments",
    auth: true,
    mustShow: ["text=Environment Registry"],
  },
  {
    name: "environments-add-modal",
    path: "/environments",
    auth: true,
    setup: async (page) => {
      await page.getByRole("button", { name: "Add Environment" }).click();
      await page.waitForTimeout(300);
    },
    mustShow: ["text=Create Environment"],
  },

  // ============================================================
  // OTHER PAGES
  // ============================================================
  {
    name: "builds",
    path: "/builds",
    auth: true,
    mustShow: ["text=Builds"],
  },
  {
    name: "cves",
    path: "/cves",
    auth: true,
    mustShow: ["text=CVE"],
  },
  {
    name: "style-guide",
    path: "/style-guide",
    auth: true,
    mustShow: ["text=Style Guide"],
  },
  {
    name: "not-found",
    path: "/not-a-real-page",
    auth: true,
    mustShow: ["text=404"],
  },
];
