// Requires failure.systemId/hostname/expectedEvents from failure-detail.py and
// the same-run Admin. Uses real UI reads; historical fixtures prove only reads.
// TASK-529 review P2 extends the existing failed History/Logs state, retaining
// the Systems design's failure identity and error-level filter semantics.
"use strict";

exports.run = async (ctx, activation) => {
  const { page, expect, receipts } = ctx;
  const systemId = receipts["failure.systemId"];
  const events = receipts["failure.expectedEvents"];
  const safe = events.find(event => event.title !== "Deployment failed").title;
  await activation.login(ctx);
  const api = `/api/v1/systems/${systemId}`;
  const read = suffix => page.waitForResponse(response =>
    new URL(response.url()).pathname === `${api}/${suffix}` && response.request().method() === "GET");
  const noPageOverflow = async () => {
    expect(await page.evaluate(() => document.documentElement.scrollWidth <=
      document.documentElement.clientWidth + 1), "long diagnostic stays within page width").toBe(true);
  };

  // Desktop reload proves persistence; narrow width discriminates overflow
  // from text that only happens to fit the standard 1440px rendering profile.
  for (const phase of ["desktop", "reload", "narrow"]) {
    if (phase === "narrow") await page.setViewportSize({ width: 375, height: 812 });
    for (const tab of ["history", "logs"]) {
      await ctx.step(`failure details: ${phase} ${tab}`, async () => {
        if (phase === "reload") {
          await page.goto(ctx.url(`/systems/${systemId}?tab=${tab}`));
        }
        const [historyResponse, logsResponse] = await Promise.all([
          read("history"), read("agent-events"),
          phase === "reload" ? page.reload() : page.goto(ctx.url(`/systems/${systemId}?tab=${tab}`)),
        ]);
        expect(historyResponse.status()).toBe(200);
        expect(logsResponse.status()).toBe(200);
        const history = await historyResponse.json();
        const logs = await logsResponse.json();
        expect(history.map(event => event.id).sort()).toEqual(events.map(event => event.id).sort());
        for (const expected of events) {
          expect(history.find(event => event.id === expected.id)).toMatchObject({
            deployment_id: expected.deploymentId, title: expected.title,
            change_reason: expected.title, event_type: "cf_deployment_failed", outcome: "failed",
          });
        }
        expect(logs).toHaveLength(events.length);
        expect(logs.map(log => log.message).sort()).toEqual(events.map(event =>
          event.title === "Deployment failed" ? event.title : `Deployment failed: ${event.title}`).sort());
        for (const log of logs) expect(log).toMatchObject({
          level: "error", event_type: "cf_deployment_failed", deployment_related: true,
        });
        await expect(page.getByRole("heading", { name: receipts["failure.hostname"], exact: true })).toBeVisible();
        if (tab === "history") {
          await expect(page.getByRole("heading", { name: "Deployment history", exact: true })).toBeVisible();
          await expect(page.locator("[data-event-id]")).toHaveCount(events.length);
          for (const event of events) {
            const row = page.locator(`[data-event-id="${event.id}"]`);
            await expect(row.getByText("Deploy failed", { exact: true })).toBeVisible();
            await expect(row.getByText("failed", { exact: true })).toBeVisible();
            await expect(row.getByText(event.title, { exact: true })).toBeVisible();
            await expect(row.getByRole("button", { name: "Rollback to this generation", exact: true })).toHaveCount(0);
            expect(await row.evaluate(node => node.scrollWidth <= node.clientWidth + 1),
              "history card contains the entire long diagnostic").toBe(true);
          }
        } else {
          const section = page.locator("section").filter({
            has: page.getByRole("heading", { name: "Deployment logs", exact: true }),
          });
          await expect(section).toBeVisible();
          // Stable event anchors identify reconstructed lines; real agent-event
          // lines are additional, unanchored entries and must not be conflated.
          await expect(section.locator('[data-ev]:not([data-ev=""])')).toHaveCount(events.length);
          for (const event of events) {
            const line = section.locator(`[data-ev="${event.id}"]`);
            await expect(line.getByText("ERROR", { exact: true })).toBeVisible();
            await expect(line).toContainText("[reconstructed timeline] deploy failed @");
            await expect(line).toContainText(event.title);
          }
          await expect(section.getByText(`Deployment failed: ${safe}`, { exact: true })).toBeVisible();
          for (const invented of ["evaluating configuration", "activation failed", "rolled back",
            "activating configuration", "build: completed", "eval: success", "generation activated"]) {
            await expect(section).not.toContainText(invented);
          }
          await section.getByRole("button", { name: "error", exact: true }).click();
          await expect(section.locator("[data-ev]")).toHaveCount(events.length * 2);
          await section.getByRole("button", { name: "info", exact: true }).click();
          await expect(section.locator("[data-ev]")).toHaveCount(0);
          await section.getByRole("button", { name: "all", exact: true }).click();
          await expect(section.locator("[data-ev]")).toHaveCount(events.length * 2);
        }
        await expect(page.locator("body")).not.toContainText(receipts["failure.rawMarker"]);
        await noPageOverflow();
        await ctx.screenshot(`delivery-failure-${phase}-${tab}`);
      });
    }
  }
  ctx.emit("failure.browserVerified", true);
};
