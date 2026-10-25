// Signs in as the bootstrapped Admin and signs out through the profile.
// Requires facts.admin and the same-run admin.userId receipt from bootstrap.
// sessions.py invokes this in a fresh context; never import bootstrap cookies.
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
    const signOut = page.getByRole("button", { name: "Sign out", exact: true });
    await expect(signOut).toBeVisible();
    // The fresh-install coach can cover the profile action. Dismiss it through
    // its real UI control, never by removing DOM or changing app preferences.
    const closeCoach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await closeCoach.isVisible()) await closeCoach.click();
    const [logout] = await Promise.all([
      page.waitForResponse((r) =>
        new URL(r.url()).pathname === "/api/auth/logout" && r.request().method() === "POST"),
      signOut.click(),
    ]);
    expect(logout.status()).toBe(204);
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
