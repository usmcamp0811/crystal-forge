// TASK-490.2: independent browser context signs in as the existing real Admin
// and signs out through the production profile. Never import bootstrap cookies.
"use strict";

exports.run = async (ctx) => {
  const { page, expect, facts, receipts } = ctx;
  await ctx.step("sign in within this independent browser context", async () => {
    await page.goto(ctx.url("/login"));
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    const who = await ctx.whoami();
    expect(who.body.is_authenticated).toBe(true);
    expect(who.body.user.id).toBe(receipts["admin.userId"]);
    const response = await page.request.get(ctx.url("/api/v1/user/sessions"));
    expect(response.status()).toBe(200);
    const current = (await response.json()).sessions.filter((s) => s.current);
    expect(current).toHaveLength(1);
    ctx.emit("sessions.browserSignOutId", current[0].id);
  });

  await ctx.step("UI sign-out ends this session", async () => {
    await page.goto(ctx.url("/profile"));
    const logout = page.waitForResponse((r) =>
      new URL(r.url()).pathname === "/api/auth/logout" && r.request().method() === "POST");
    await page.getByRole("button", { name: "Sign out", exact: true }).click();
    expect((await logout).status()).toBe(204);
    await expect(page).toHaveURL(/\/login$/);
    const who = await ctx.whoami();
    expect(who.body.is_authenticated).toBe(false);
  });

  await ctx.step("reload and protected navigation cannot reuse the session", async () => {
    await page.reload();
    await expect(page).toHaveURL(/\/login$/);
    expect((await ctx.whoami()).body.is_authenticated).toBe(false);
    const protectedRequest = await page.request.get(ctx.url("/api/v1/user/sessions"));
    expect(protectedRequest.status()).toBe(401);
    await page.goto(ctx.url("/profile"));
    await expect(page).toHaveURL(/\/login$/);
    await expect(page.getByRole("button", { name: "Sign In", exact: true })).toBeVisible();
    await ctx.screenshot("signed-out-protected-navigation");
  });
};
