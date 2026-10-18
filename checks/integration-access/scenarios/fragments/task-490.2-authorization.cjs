// TASK-490.2: an independent browser context signs in as the Viewer that the
// Admin API created, then looks for administrative surfaces through the
// production UI. It never imports cookies from another context and it changes
// no application code, DOM element, selector, or response.
//
// Requirement (approved design, "Server Management" page): the page is
// "Admin-only". The production UI therefore hides the administration entry and
// shows "Access Denied" at /admin for a non-Admin. This spec asserts exactly
// that. It does not claim anything about pages a Viewer may legitimately read.
// Server-side enforcement is asserted separately by the API cases, because
// hiding a control alone proves nothing about the server.
"use strict";

// Labels of the Admin page's mutation controls in the production UI.
const ADMIN_MUTATION_BUTTONS = /^(Add user|Delete user|Save changes|Save retry config|Save banner config|Save heartbeat config)$/;

exports.run = async (ctx) => {
  const { page, expect, facts, receipts } = ctx;
  const viewer = facts.viewer;

  // Record every non-GET request to an administrative API. The UI MUST NOT
  // issue one on behalf of a Viewer.
  const adminMutations = [];
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (path.startsWith("/api/v1/admin/") && request.method() !== "GET") {
      adminMutations.push(`${request.method()} ${path}`);
    }
  });

  await ctx.step("Viewer signs in through the production login", async () => {
    await page.goto(ctx.url("/login"));
    // The server accepts a username or an email in the same field.
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(viewer.email);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(viewer.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    const who = await ctx.whoami();
    expect(who.body.is_authenticated).toBe(true);
    expect(who.body.user.id).toBe(receipts["authorization.viewer.userId"]);
    expect(who.body.roles).toEqual([viewer.roleWire]);
    ctx.emit("authorization.viewer.browserUserId", who.body.user.id);
  });

  await ctx.step("navigation offers no administration entry", async () => {
    // Wait for a link every role sees, so absence below is not just "not yet
    // rendered".
    await expect(page.getByRole("link", { name: "Systems", exact: true })).toBeVisible();
    await expect(page.locator('a[href="/admin"]')).toHaveCount(0);
  });

  await ctx.step("direct /admin shows Access Denied and no mutation control", async () => {
    await page.goto(ctx.url("/admin"));
    await expect(page.getByRole("heading", { name: "Access Denied", exact: true })).toBeVisible();
    await expect(page.getByText("This page requires an administrator role.", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: ADMIN_MUTATION_BUTTONS })).toHaveCount(0);
    await expect(page.locator('a[href="/admin"]')).toHaveCount(0);
    await ctx.screenshot("viewer-admin-access-denied");
  });

  await ctx.step("the UI issued no administrative mutation request", async () => {
    expect(adminMutations).toEqual([]);
    // The denied view stays after a reload: it does not depend on transient state.
    await page.reload();
    await expect(page.getByRole("heading", { name: "Access Denied", exact: true })).toBeVisible();
    expect(adminMutations).toEqual([]);
  });
};
