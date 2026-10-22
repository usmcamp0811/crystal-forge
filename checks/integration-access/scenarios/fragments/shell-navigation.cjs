// Same-run Admin only. Python requests one phase; every browser run signs in
// independently. Shell.jsx owns primary navigation labels; routes.rs supplies
// their compatible local-auth paths. Detail routes require domain fixtures and
// are not primary-navigation destinations. No successful response is mocked.
"use strict";

const phases = ["protected", "routes", "unknown", "keyboard", "mobile", "sidebarPersistence"];
const destinations = [
  ["Dashboard", "/", "Dashboard"],
  ["Systems", "/systems", "Systems"],
  ["Flakes", "/flakes", "Flakes"],
  ["Environments", "/environments", "Environments"],
  ["Evaluations", "/evaluations", "Evaluations"],
  ["Builds", "/builds", "Builds"],
  ["Scanning", "/scanning", "Scanning"],
  ["CVEs", "/cves", "CVEs"],
  ["Policies", "/deployment-policies", "Policies"],
  ["Bundles", "/compliance", "Compliance"],
  ["POA&M", "/poams", "POA&M"],
  ["Builders", "/builders", "Builders"],
  ["Caches", "/caches", "Caches"],
  ["Server", "/admin", "Server Management"],
];

exports.run = async (ctx) => {
  const { page, expect, facts, receipts } = ctx;
  const pending = phases.filter((p) => receipts[`shell.request.${p}`] && !receipts[`shell.browser.${p}`]);
  expect(pending).toHaveLength(1);
  const phase = pending[0];
  const sidebar = page.getByTestId("sidebar-nav");
  const heading = (name) => page.getByRole("heading", { name, exact: true });
  // Match href rather than accessible names: badges append counts and the rail
  // intentionally omits labels. Each href is unique within the selected nav.
  const link = (scope, path) => scope.locator(`a[href="${path}"],a[href^="${path}?"]`);
  const atPath = (path) => expect(page).toHaveURL((url) => url.pathname === path);
  const dismissCoach = async () => {
    const close = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await close.isVisible()) await close.click();
  };

  await ctx.step(`${phase}: establish independent authentication`, async () => {
    if (phase === "protected") {
      await page.goto(ctx.url("/login"));
      expect((await ctx.whoami()).body.is_authenticated).toBe(false);
      return;
    }
    await page.goto(ctx.url("/login"));
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    expect((await ctx.whoami()).body.user.id).toBe(receipts["admin.userId"]);
    await expect(heading("Dashboard")).toBeVisible();
    await dismissCoach();
  });

  await ctx.step(`${phase}: real shell interaction`, async () => {
    if (phase === "protected") {
      for (const path of [...destinations.map((d) => d[1]), "/profile", "/access-unknown-route"]) {
        await page.goto(ctx.url(path));
        await expect(page).toHaveURL(ctx.url("/login"));
        await expect(page.getByRole("button", { name: "Sign In", exact: true })).toBeVisible();
        await expect(sidebar).toHaveCount(0);
      }
      expect((await ctx.whoami()).body.is_authenticated).toBe(false);
    } else if (phase === "routes") {
      for (const [label, path, title] of destinations) {
        await expect(link(sidebar, path)).toHaveCount(1);
        await link(sidebar, path).click();
        await atPath(path);
        await expect(heading(title)).toBeVisible();
        await dismissCoach();
        ctx.emit(`shell.route.${label}`, { path, heading: title });
      }
      await link(sidebar, "/profile").click();
      await expect(page).toHaveURL(ctx.url("/profile"));
      await expect(heading("Profile & Preferences")).toBeVisible();
    } else if (phase === "unknown") {
      await page.goto(ctx.url("/access-unknown-route"));
      await expect(heading("404")).toBeVisible();
      await expect(page.getByText("Page not found: /access-unknown-route", { exact: true })).toBeVisible();
      await page.getByRole("link", { name: "Go to Dashboard", exact: true }).click();
      await expect(page).toHaveURL(ctx.url("/"));
      await expect(heading("Dashboard")).toBeVisible();
    } else if (phase === "keyboard") {
      await page.goto(ctx.url("/profile"));
      await expect(heading("Profile & Preferences")).toBeVisible();
      await dismissCoach();
      // A real Tab traversal, never focus() or click(). The bound is the DOM's
      // focusable candidate count plus one wrap, not a timing-dependent sleep.
      const target = link(sidebar, "/systems");
      const bound = await page.locator("a[href],button,input,select,textarea,[tabindex]").count() + 1;
      for (let i = 0; i < bound; i++) {
        await page.keyboard.press("Tab");
        if (await target.evaluate((el) => el === document.activeElement)) break;
      }
      await expect(target).toBeFocused();
      await page.keyboard.press("Enter");
      await atPath("/systems");
      await expect(heading("Systems")).toBeVisible();
    } else if (phase === "mobile") {
      expect(page.viewportSize().width).toBeLessThan(768);
      await expect(sidebar).toBeHidden();
      const menu = page.getByRole("button", { name: "Open navigation menu", exact: true });
      const drawer = page.getByTestId("mobile-drawer");
      await menu.click();
      await expect(drawer).toBeVisible();
      const close = drawer.getByRole("button", { name: "Close menu", exact: true });
      await expect(close).toBeVisible();
      await expect(close).toBeEnabled();
      // Existing p-2 and w-6/h-6 intent: 8px padding around a 24px icon.
      // Verify a usable hit target, not merely a visible outer drawer.
      await expect(close).toHaveCSS("padding", "8px");
      await expect(close.locator("svg")).toHaveCSS("width", "24px");
      await expect(close.locator("svg")).toHaveCSS("height", "24px");
      await expect.poll(() => close.evaluate((el) => {
        const r = el.getBoundingClientRect();
        const hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
        return r.width === 40 && r.height === 40
          && r.left >= 0 && r.top >= 0
          && r.right <= window.innerWidth && r.bottom <= window.innerHeight
          && !!hit && el.contains(hit);
      })).toBe(true);
      await close.click();
      await expect(drawer).toHaveCount(0);
      await expect(page.getByTestId("mobile-drawer-backdrop")).toHaveCount(0);
      await expect(menu).toBeVisible();
      await menu.click();
      await expect(drawer).toBeVisible();
      const backdrop = page.getByTestId("mobile-drawer-backdrop");
      await expect(backdrop).toBeVisible();
      // Click the exposed overlay outside the drawer through normal hit testing.
      const drawerBounds = await drawer.boundingBox();
      const backdropBounds = await backdrop.boundingBox();
      expect(drawerBounds).not.toBeNull();
      expect(backdropBounds).not.toBeNull();
      const overlayX = drawerBounds.x + drawerBounds.width
        + (page.viewportSize().width - drawerBounds.x - drawerBounds.width) / 2;
      await backdrop.click({ position: {
        x: overlayX - backdropBounds.x,
        y: backdropBounds.height / 2,
      } });
      await expect(drawer).toHaveCount(0);
      await expect(backdrop).toHaveCount(0);
      await menu.click();
      await expect(drawer).toBeVisible();
      await link(drawer, "/systems").click();
      await atPath("/systems");
      await expect(heading("Systems")).toBeVisible();
      await expect(drawer).toHaveCount(0);
    } else if (phase === "sidebarPersistence") {
      // Normalize only through the UI, preserving the independent context's
      // initial server state until preference bootstrap has completed.
      let current;
      await expect.poll(async () => {
        const loaded = await page.request.get(ctx.url("/api/v1/user/preferences"));
        expect(loaded.status()).toBe(200);
        current = (await loaded.json()).preferences;
        return current;
      }).not.toBeNull();
      await expect(sidebar.getByRole("button", { name: current.sidebar_collapsed ? "Expand sidebar" : "Collapse sidebar", exact: true })).toBeVisible();
      const toggle = async (name) => {
        const [response] = await Promise.all([
          page.waitForResponse((r) => new URL(r.url()).pathname === "/api/v1/user/preferences" && r.request().method() === "PATCH"),
          sidebar.getByRole("button", { name, exact: true }).click(),
        ]);
        expect(response.status()).toBe(200);
        return (await response.json()).preferences;
      };
      if (current.sidebar_collapsed) await toggle("Expand sidebar");
      expect((await toggle("Collapse sidebar")).sidebar_collapsed).toBe(true);
      await page.reload();
      await expect(sidebar.getByRole("button", { name: "Expand sidebar", exact: true })).toBeVisible();
    }
    await ctx.screenshot(`shell-${phase}`);
    ctx.emit(`shell.browser.${phase}`, true);
  });
};
