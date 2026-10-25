// Final same-run continuity read. Backend/target receipts precede browser proof;
// no browser result substitutes for actual activation or signed state reports.
"use strict";
exports.run = async ctx => {
  const {page, expect, facts, receipts} = ctx;
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", {exact: true}).fill(facts.admin.username);
  await page.getByPlaceholder("Enter your password", {exact: true}).fill(facts.admin.password);
  await page.getByRole("button", {name: "Sign In", exact: true}).click();
  await expect(page).toHaveURL(ctx.url("/"));
  const who = await ctx.whoami();
  expect(who.status).toBe(200);
  expect(who.body.user.id).toBe(receipts["upgrade.adminId"]);
  const coach = page.getByRole("button", {name: "Close Crystal Forge Coach", exact: true});
  if (await coach.isVisible()) await coach.click();
  const path = `/api/v1/systems/${receipts["upgrade.targetId"]}/history`;
  const [response] = await Promise.all([
    page.waitForResponse(r => new URL(r.url()).pathname === path && r.request().method() === "GET"),
    page.goto(ctx.url(`/systems/${receipts["upgrade.targetId"]}?tab=history`)),
  ]);
  expect(response.status()).toBe(200);
  const history = await response.json();
  // Legacy signed observations are not certified CF deployment events. Their
  // SQL identities are checked by the driver; the normal observation API keeps
  // their original meaning instead of inventing successful deployments.
  const observedResponse = await page.request.get(ctx.url(
    `/api/v1/systems/${receipts["upgrade.targetId"]}/agent-events`));
  expect(observedResponse.status()).toBe(200);
  const observations = await observedResponse.json();
  for (const row of receipts["upgrade.completedHistory"].filter(r => r.change_reason !== "state_delta")) {
    expect(observations.some(e => e.event_type === "state_change" && e.message.includes(row.store_path)
      && Date.parse(e.timestamp) === Date.parse(row.timestamp)),
      "original predecessor observation remains readable with its path/time provenance").toBe(true);
  }
  const request = receipts["upgrade.current.request"];
  expect(history.filter(e => e.deployment_id === request.deployment_id && e.event_type === "cf_deployment_succeeded")).toHaveLength(1);
  await expect(page.getByRole("heading", {name: "targetA", exact: true})).toBeVisible();
  await expect(page.getByRole("heading", {name: "Deployment history", exact: true})).toBeVisible();
  await ctx.screenshot("upgrade-preserved-history-current-operation");
  ctx.emit("upgrade.browserContinuityVerified", true);
};
