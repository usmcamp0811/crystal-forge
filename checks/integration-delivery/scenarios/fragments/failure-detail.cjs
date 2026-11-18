// Requires same-run failure records/role identities from failure-detail.py.
// TASK-529 expands the existing Systems failure state without a visual redesign.
// Retain membership/severity/no-invented-steps/reload/overflow assertions; replace
// the old two-lines-per-failure expectation with the approved exact-event rule.
"use strict";
const fs = require("node:fs");
const { actors } = require("/etc/cf-it/failure-report.cjs");

function expectedLogSummary(view) {
  if (view.summary.startsWith("Deployment failed")) {
    return view.stage === "Deployment" ? view.summary : `${view.stage}: ${view.summary}`;
  }
  return `${view.stage} failed: ${view.summary}`;
}

exports.run = async (ctx, activation) => {
  const { page, expect, receipts } = ctx;
  const systemId = receipts["failure.systemId"];
  const events = receipts["failure.expectedEvents"];
  const api = `/api/v1/systems/${systemId}`;
  const read = async suffix => {
    // Use the real signed-in browser context's cookies. Polling responses from
    // an outgoing document can lose their CDP bodies during navigation; explicit
    // API responses have an independent lifetime. Rendered fields below still
    // prove that the production UI reads and displays this role's projection.
    const response = await page.request.get(ctx.url(`${api}/${suffix}`));
    try {
      expect(response.status(), `authorized GET ${suffix}`).toBe(200);
      return await response.json();
    } finally {
      await response.dispose();
    }
  };
  // CONTRACT: queries/system_events.rs keeps a failed request visible to
  // deployment-status for two minutes after completion, then returns 204. Both
  // states are asserted; neither is tolerated as an unexplained difference.
  const readProgress = async () => {
    const response = await page.request.get(ctx.url(`${api}/deployment-status`));
    try {
      const status = response.status();
      expect([200, 204], "authorized GET deployment-status").toContain(status);
      return status === 200 ? await response.json() : null;
    } finally {
      await response.dispose();
    }
  };
  const noOverflow = async locator => {
    await expect.poll(() => locator.evaluate(node => node.scrollWidth <= node.clientWidth + 1),
      { message: "approved diagnostics stay within the available width" }).toBe(true);
  };
  const assertFields = async (container, view) => {
    for (const value of [view.stage, view.summary, view.reason_status, view.code, view.remediation_hint]) {
      if (value !== undefined) await expect(container.getByText(value, { exact: true })).toBeVisible();
    }
    for (const label of ["Stage", "Summary", "Reason status"]) {
      await expect(container.getByText(label, { exact: true })).toBeVisible();
    }
    for (const [field, label] of [["code", "Code"], ["remediation_hint", "Remediation"],
      ["exit_code", "Exit code"], ["attempt_count", "Attempts"], ["max_attempts", "Maximum attempts"]]) {
      await expect(container.getByText(label, { exact: true })).toHaveCount(view[field] === undefined ? 0 : 1);
      if (view[field] !== undefined) {
        const fact = container.locator("dl > div").filter({ has: page.getByText(label, { exact: true }) });
        await expect(fact.locator("dd")).toHaveText(String(view[field]));
      }
    }
    const details = container.getByText("Diagnostic details", { exact: true });
    await expect(details).toHaveCount(view.diagnostics ? 1 : 0);
    if (view.diagnostics) {
      await details.click();
      for (const line of view.diagnostics.lines) await expect(container.getByText(line, { exact: true })).toBeVisible();
      if (view.diagnostics.cache_endpoint) {
        await expect(container.getByText(`Cache endpoint: ${view.diagnostics.cache_endpoint}`, { exact: true })).toBeVisible();
      }
      if (view.diagnostics.suppression_reason) {
        await expect(container.getByText(`Diagnostics suppressed: ${view.diagnostics.suppression_reason}`, { exact: true })).toBeVisible();
      }
    }
  };
  const assertLog = async (line, view) => {
    await expect(line.getByText("ERROR", { exact: true })).toBeVisible();
    await expect(line).toContainText(expectedLogSummary(view));
    await expect(line).not.toContainText("failed: Deployment failed");
    await expect(line).toContainText(`Reason status: ${view.reason_status}`);
    for (const [field, label] of [["code", "Code"], ["remediation_hint", "Remediation"],
      ["exit_code", "Exit code"], ["attempt_count", "Attempts"], ["max_attempts", "Maximum attempts"]]) {
      if (view[field] !== undefined) await expect(line).toContainText(`${label}: ${view[field]}`);
      else await expect(line).not.toContainText(`${label}:`);
    }
    for (const diagnostic of view.diagnostics?.lines || []) {
      await expect(line).toContainText(diagnostic);
      // Neither the server summary nor a second reconstructed line duplicates details.
      expect((await line.textContent()).split(diagnostic)).toHaveLength(2);
    }
    if (view.diagnostics?.cache_endpoint) await expect(line).toContainText(view.diagnostics.cache_endpoint);
    if (view.diagnostics?.suppression_reason) await expect(line).toContainText(view.diagnostics.suppression_reason);
    await expect(line).not.toContainText("[reconstructed timeline]");
  };
  const assertSafe = async (role, text) => {
    for (const forbidden of [receipts["failure.rawMarker"], "END-LEGACY-DISPLAY", "future_unknown_key"]) {
      expect(text).not.toContain(forbidden);
    }
    if (role !== "Admin") {
      expect(text).not.toContain("END-APPROVED-DIAGNOSTIC");
      expect(text).not.toContain("cache.example.test");
      expect(text).not.toContain("Diagnostic details");
    }
    if (role === "Viewer") {
      expect(text).not.toContain("Check DNS configuration");
      expect(text).not.toContain("Exit code:");
    }
  };

  // A full navigation after cookie clearing reinitializes app authentication.
  // The production login and exact whoami identity prove the selected actor.
  const loginAs = async role => {
    // The shared Admin helper asserts a sidebar link named exactly "Systems".
    // That link's accessible name gains an attention badge ("Systems 1") once
    // that asynchronous state loads, so the helper only passes if it wins a
    // race. This spec signs in several times and must not depend on that race.
    // The previous role's last phase leaves a 375px viewport, where the sidebar
    // is collapsed. Sign in at desktop width so state never leaks between roles.
    await page.setViewportSize({ width: 1440, height: 1000 });
    await ctx.context.clearCookies();
    await page.goto(ctx.url("/login"));
    const admin = role === "Admin";
    const username = admin ? ctx.facts.admin.username : actors.find(candidate => candidate.role === role).email;
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(ctx.facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    // The exact identity and roles prove the selected actor, not the UI.
    const who = await ctx.whoami();
    expect(who.status).toBe(200);
    expect(who.body.is_authenticated).toBe(true);
    expect(who.body.user.id).toBe(admin ? receipts["delivery.adminId"] : receipts[`failure.${role}.userId`]);
    expect(who.body.roles).toEqual([role]);
    await expect(page.getByRole("link", { name: /^Systems( \d+)?$/ }).first()).toBeVisible();
    const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await coach.isVisible()) await coach.click();
  };
  // Strict field-by-field check of the progress banner against the role's server
  // projection. Only valid while the two-minute window is certainly open.
  const assertBanner = async (role, progress) => {
    const failed = events.find(event => event.deploymentId === progress.id);
    expect(failed, "progress identifies a recorded failed deployment").toBeDefined();
    await assertFields(page.locator(`[data-deployment-progress="${progress.id}"]`), failed.views[role]);
  };
  // Later phases can straddle the window's expiry, so a retrying field assertion
  // may lose its element mid-check. Take one atomic DOM snapshot of whatever
  // banner is displayed and verify it is safe for the role; its exact fields are
  // asserted strictly in the pre-pass above.
  const assertBannerSnapshot = async role => {
    const texts = await page.evaluate(() =>
      [...document.querySelectorAll("[data-deployment-progress]")].map(node => node.textContent));
    for (const text of texts) await assertSafe(role, text);
  };

  // The roles run one after another and the long screenshot phases below can
  // outlast the two-minute banner window. Verify every role's banner first, while
  // the window is certain to be open, so banner coverage never depends on speed.
  for (const role of ["Admin", "Operator", "Viewer"]) {
    await loginAs(role);
    await page.setViewportSize({ width: 1440, height: 1000 });
    await ctx.step(`failure progress banner: ${role}`, async () => {
      await page.goto(ctx.url(`/systems/${systemId}?tab=history`));
      const progress = await readProgress();
      expect(progress, "failed deployment must still be inside its two-minute progress window").not.toBeNull();
      await assertSafe(role, JSON.stringify(progress));
      await assertBanner(role, progress);
    });
  }

  for (const role of ["Admin", "Operator", "Viewer"]) {
    await loginAs(role);
    await page.setViewportSize({ width: 1440, height: 1000 });
    for (const phase of ["desktop", "reload", "narrow"]) {
      if (phase === "narrow") await page.setViewportSize({ width: 375, height: 812 });
      for (const tab of ["history", "logs"]) {
        await ctx.step(`failure details: ${role} ${phase} ${tab}`, async () => {
          if (phase === "reload") {
            const selectedTab = page.getByRole("tab", { name: tab === "history" ? "History" : "Logs", exact: true });
            await selectedTab.click();
            await expect(selectedTab).toHaveAttribute("aria-selected", "true");
            await expect(page).toHaveURL(url => url.pathname === `/systems/${systemId}` && url.searchParams.get("tab") === tab);
            await page.reload();
          } else {
            await page.goto(ctx.url(`/systems/${systemId}?tab=${tab}`));
          }
          const [history, logs, progress] = await Promise.all([
            read("history"), read("agent-events"), readProgress(),
          ]);
          expect(history.map(event => event.id).sort()).toEqual(events.map(event => event.id).sort());
          const failures = logs.filter(event => event.event_type === "cf_deployment_failed");
          expect(failures).toHaveLength(events.length);
          expect(logs).toHaveLength(events.length + 2);
          for (const expected of events) {
            expect(history.find(event => event.id === expected.id)).toMatchObject({
              deployment_id: expected.deploymentId, title: expected.messages[role],
              change_reason: expected.messages[role], event_type: "cf_deployment_failed", outcome: "failed",
              failure: expected.views[role],
            });
            const log = failures.find(event => event.event_id === expected.id);
            expect(log.deployment_id ?? null).toBe(expected.deploymentId);
            expect(log).toMatchObject({
              message: expected.messages[role],
              level: "error", deployment_related: true, failure: expected.views[role],
            });
          }
          await assertSafe(role, JSON.stringify({ history, logs, progress }));
          const systemHeading = page.getByRole("heading", { name: receipts["failure.hostname"], exact: true });
          await expect(systemHeading).toBeVisible();
          if (tab === "history") {
            await systemHeading.evaluate(node => node.scrollIntoView({ block: "center", inline: "nearest" }));
            await expect(systemHeading).toBeInViewport({ ratio: 1 });
            await ctx.screenshot(`delivery-failure-${role}-${phase}-context`);
          }
          await assertBannerSnapshot(role);
          if (tab === "history") {
            await expect(page.getByRole("heading", { name: "Deployment history", exact: true })).toBeVisible();
            await expect(page.locator("[data-event-id]")).toHaveCount(events.length);
            for (const event of events) {
              const row = page.locator(`[data-event-id="${event.id}"]`);
              await expect(row.getByText("Deploy failed", { exact: true })).toBeVisible();
              await expect(row.getByText("failed", { exact: true })).toBeVisible();
              await assertFields(row, event.views[role]);
              await expect(row.getByRole("button", { name: "Rollback to this generation", exact: true })).toHaveCount(0);
              await noOverflow(row);
            }
            // Exact persisted identity, including retries of the same commit,
            // must survive the History-to-Logs action and land on one real line.
            for (const event of events.filter(event => ["dns", "dns-repeat"].includes(event.label))) {
              await page.locator(`[data-event-id="${event.id}"]`).getByRole("button", {
                name: "Jump to this event in logs", exact: true,
              }).click();
              await expect(page.getByRole("heading", { name: "Deployment logs", exact: true })).toBeVisible();
              const line = page.locator(`[data-ev="${event.id}"]`);
              await expect(line).toHaveCount(1);
              await expect(line).toBeInViewport();
              await expect(page.getByRole("checkbox", { name: "auto-scroll", exact: true })).not.toBeChecked();
              await assertLog(line, event.views[role]);
              await page.getByRole("tab", { name: "History", exact: true }).click();
              await expect(page.getByRole("heading", { name: "Deployment history", exact: true })).toBeVisible();
            }
          } else {
            const section = page.locator("section").filter({ has: page.getByRole("heading", { name: "Deployment logs", exact: true }) });
            await expect(section).toBeVisible();
            await expect(section.locator("[data-ev]")).toHaveCount(events.length + 2);
            for (const event of events) {
              const line = section.locator(`[data-ev="${event.id}"]`);
              await expect(line).toHaveCount(1);
              await assertLog(line, event.views[role]);
            }
            await expect(section).toContainText("Agent started");
            await expect(section).toContainText("agent heartbeat version=failure-probe-heartbeat");
            for (const invented of ["evaluating configuration", "activation failed", "rolled back", "activating configuration", "build: completed", "eval: success", "generation activated"]) {
              await expect(section).not.toContainText(invented);
            }
            await section.getByRole("button", { name: "error", exact: true }).click();
            await expect(section.locator("[data-ev]")).toHaveCount(events.length);
            const download = await ctx.download(`failure-${role}-${phase}`, () => section.getByRole("button", { name: "Download", exact: true }).click());
            const text = fs.readFileSync(download.path, "utf8");
            for (const event of events) {
              const view = event.views[role];
              expect(text).toContain(expectedLogSummary(view));
              expect(text).toContain(`Reason status: ${view.reason_status}`);
              for (const [field, label] of [["code", "Code"], ["remediation_hint", "Remediation"],
                ["exit_code", "Exit code"], ["attempt_count", "Attempts"], ["max_attempts", "Maximum attempts"]]) {
                if (view[field] !== undefined) expect(text).toContain(`${label}: ${view[field]}`);
              }
              for (const diagnostic of view.diagnostics?.lines || []) expect(text).toContain(diagnostic);
              if (view.diagnostics?.cache_endpoint) expect(text).toContain(view.diagnostics.cache_endpoint);
              if (view.diagnostics?.suppression_reason) expect(text).toContain(`Diagnostics suppressed: ${view.diagnostics.suppression_reason}`);
            }
            expect(text.match(/\[ERROR\]/g)).toHaveLength(events.length);
            expect(text).not.toContain("failed: Deployment failed");
            expect(text).not.toContain("agent heartbeat");
            await assertSafe(role, text);
            await section.getByRole("button", { name: "info", exact: true }).click();
            await expect(section.locator("[data-ev]")).toHaveCount(2);
            await section.getByRole("button", { name: "all", exact: true }).click();
            await expect(section.locator("[data-ev]")).toHaveCount(events.length + 2);
            await noOverflow(section);
          }
          await assertSafe(role, await page.locator("body").textContent());
          await noOverflow(page.locator("html"));
          // Frame the actual tab content rather than the preceding progress
          // banner. Keep system identity in the separate context image above.
          const representative = events.find(event => event.label === "incomplete_capture");
          const contentRow = page.locator(tab === "history"
            ? `[data-event-id="${representative.id}"]`
            : `[data-ev="${representative.id}"]`);
          if (tab === "logs") {
            await page.getByRole("checkbox", { name: "auto-scroll", exact: true }).uncheck();
          }
          // A whole stacked History card can exceed its timeline's height;
          // frame its useful summary rather than requiring the entire card to fit.
          const contentFocus = tab === "history"
            ? contentRow.getByText(representative.views[role].summary, { exact: true })
            : contentRow;
          await contentFocus.evaluate(node => node.scrollIntoView({ block: "center", inline: "nearest" }));
          await expect(contentFocus).toBeInViewport({ ratio: 1 });
          await ctx.screenshot(`delivery-failure-${role}-${phase}-${tab}`);
          if (role === "Admin") {
            const diagnosticEvent = events.find(event => event.label === "dns");
            const diagnosticRow = page.locator(tab === "history"
              ? `[data-event-id="${diagnosticEvent.id}"]`
              : `[data-ev="${diagnosticEvent.id}"]`);
            if (tab === "history") {
              // History-to-Logs round trips remount the cards with disclosures
              // closed. Reopen this real row before capturing diagnostic evidence.
              const details = diagnosticRow.locator("details");
              if (!await details.evaluate(node => node.open)) {
                await diagnosticRow.getByText("Diagnostic details", { exact: true }).click();
              }
              const diagnostic = diagnosticRow.getByText(diagnosticEvent.views.Admin.diagnostics.lines[0], { exact: true });
              // Center the diagnostic inside both scroll containers. End
              // alignment can clip a fractional bottom pixel at scroll limits.
              await diagnostic.evaluate(node => node.scrollIntoView({ block: "center", inline: "nearest" }));
              await expect(diagnostic).toBeInViewport({ ratio: 1 });
            } else {
              // Diagnostics end the projected log message; end alignment keeps
              // them visible even when a narrow wrapped row exceeds stream height.
              await diagnosticRow.evaluate(node => node.scrollIntoView({ block: "end", inline: "nearest" }));
              await expect(diagnosticRow).toBeInViewport();
              await assertLog(diagnosticRow, diagnosticEvent.views.Admin);
            }
            await noOverflow(diagnosticRow);
            await ctx.screenshot(`delivery-failure-${role}-${phase}-${tab}-diagnostics`);
          }
        });
      }
    }
  }
  ctx.emit("failure.browserVerified", true);
};
