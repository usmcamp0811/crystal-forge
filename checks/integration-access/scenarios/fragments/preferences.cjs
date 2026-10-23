// ProfileView.jsx controls, compatible real local-auth API and same-run u1/u2.
// Every named phase is a fresh browser context. Only deliberate transport
// failures/delays are intercepted; all successful responses reach the server.
"use strict";

const phases = ["baseline-a", "baseline-b", "appearance", "notifications", "verify-b",
  "appearance-read-failure", "appearance-save-failure", "notification-read-failure",
  "notification-save-failure", "notification-loading"];
const appearancePath = "/api/v1/user/preferences";
const notificationPath = "/api/v1/user/notification-preferences";
const notificationLabels = {
  deploy_failures: "Deploy failures", build_failures: "Build failures",
  critical_cves: "New critical CVEs", policy_violations: "Policy violations",
  heartbeat_lost: "Heartbeat lost", weekly_digest: "Weekly digest email",
};
const appearanceOptions = {
  theme: { dark: "Dark", light: "Light" },
  density: { comfortable: "Comfort", compact: "Compact" },
  sidebar_collapsed: { false: "Full", true: "Rail" },
  default_systems_view: { cards: "Cards", table: "Table" },
};
const channelLabels = { in_app: "In-app", email: "Email", both: "Both" };

exports.run = async (ctx) => {
  const { page, expect, facts, receipts } = ctx;
  const pending = phases.filter((p) => receipts[`preferences.request.${p}`] && !receipts[`preferences.browser.${p}`]);
  expect(pending).toHaveLength(1);
  const phase = pending[0];
  const isB = phase === "baseline-b" || phase === "verify-b";
  const actor = isB ? facts.operator : facts.admin;
  const userId = receipts[isB ? "authorization.operator.userId" : "admin.userId"];
  const config = facts.preferences;
  const heading = () => page.getByRole("heading", { name: "Profile & Preferences", exact: true });
  // PrefRow's title is not an input label. Scope via the exact visible title's
  // grandparent, the row container, then use the checkbox's semantic role.
  const checkbox = (key) => page.getByText(notificationLabels[key], { exact: true })
    .locator("../..").getByRole("checkbox");
  const activeButton = async (label) => {
    await expect(page.getByRole("button", { name: label, exact: true })).toHaveClass(/\bactive\b/);
  };
  const projectNotifications = (body) => Object.fromEntries(
    [...Object.keys(notificationLabels), "delivery_channel"].map((k) => [k, body[k]]));
  const projectAppearance = (body) => Object.fromEntries(
    Object.keys(appearanceOptions).map((k) => [k, body.preferences[k]]));
  const assertIdle = async (path, checkpoint) => {
    const patches = [];
    const observe = (r) => {
      if (new URL(r.url()).pathname === path && r.method() === "PATCH") patches.push(r.method());
    };
    page.on("request", observe);
    try {
      // A bounded observation window detects self-sustaining reactive saves.
      // This is the behavior under test, not a delay before a readiness check.
      await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 1500)));
      expect(patches).toEqual([]);
      ctx.emit(`preferences.idle.${phase}.${checkpoint}`, { milliseconds: 1500, patches: 0 });
    } finally { page.off("request", observe); }
  };
  const get = async (path) => {
    const response = await page.request.get(ctx.url(path));
    expect(response.status()).toBe(200);
    return response.json();
  };
  const assertAppearance = async (values) => {
    for (const [key, options] of Object.entries(appearanceOptions)) {
      expect(Object.hasOwn(options, String(values[key]))).toBe(true);
      await activeButton(options[String(values[key])]);
    }
  };
  const assertNotifications = async (values) => {
    for (const key of Object.keys(notificationLabels)) {
      await expect(checkbox(key)).toBeChecked({ checked: values[key] });
    }
    await activeButton(channelLabels[values.delivery_channel]);
  };
  const dismissCoach = async () => {
    const close = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await close.isVisible()) await close.click();
  };
  const save = async (path, trigger, expected) => {
    const [response] = await Promise.all([
      page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().method() === "PATCH"),
      trigger(),
    ]);
    expect(response.status()).toBe(200);
    const body = await response.json();
    const values = path === appearancePath ? body.preferences : body;
    for (const [key, value] of Object.entries(expected)) expect(values[key]).toEqual(value);
    return values;
  };
  const unchangedSelection = async (path, key, target, label) => {
    const before = await get(path);
    const project = path === appearancePath ? projectAppearance : projectNotifications;
    const values = path === appearancePath ? before.preferences : before;
    expect(values[key]).toEqual(target);
    await activeButton(label);
    const patches = [];
    const observe = (request) => {
      if (new URL(request.url()).pathname === path && request.method() === "PATCH") {
        patches.push(request.method());
      }
    };
    // Selected segments may invoke the save callback. The contract is unchanged
    // state, not transport deduplication. Changed values still require the real
    // PATCH response above; no synthetic response can establish persistence.
    page.on("request", observe);
    try {
      await page.getByRole("button", { name: label, exact: true }).click();
      await activeButton(label);
      expect(project(await get(path))).toEqual(project(before));
      await page.reload();
      await expect(heading()).toBeVisible();
      await assertAppearance((await get(appearancePath)).preferences);
      await assertNotifications(projectNotifications(await get(notificationPath)));
      expect(project(await get(path))).toEqual(project(before));
      expect(patches.length).toBeLessThanOrEqual(1);
      await assertIdle(path, `selected-${key}`);
      ctx.emit(`preferences.noop.${phase}.${key}`, true);
    } finally {
      page.off("request", observe);
    }
  };

  await ctx.step(`${phase}: independent ${isB ? "Operator" : "Admin"} sign-in`, async () => {
    await page.goto(ctx.url("/login"));
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(actor.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(actor.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    const who = await ctx.whoami();
    expect(who.body.is_authenticated).toBe(true);
    expect(who.body.user.id).toBe(userId);
    await expect(page.getByRole("heading", { name: "Dashboard", exact: true })).toBeVisible();
    await dismissCoach();
    await page.goto(ctx.url("/profile"));
    await expect(heading()).toBeVisible();
    await dismissCoach();
    await expect(page.getByRole("main").getByText(actor.email, { exact: true })).toBeVisible();
    // Active controls prove asynchronous preference bootstrap applied before
    // mutations. Reading the API alone does not prove the UI loaded its state.
    await expect.poll(async () => (await get(appearancePath)).preferences).not.toBeNull();
    const loaded = (await get(appearancePath)).preferences;
    await assertAppearance(loaded);
    await expect(checkbox("deploy_failures")).toBeVisible();
  });

  await ctx.step(`${phase}: controls, real responses and reload`, async () => {
    if (isB || phase === "baseline-a") {
      const appearance = (await get(appearancePath)).preferences;
      const notifications = projectNotifications(await get(notificationPath));
      await assertAppearance(appearance);
      await assertNotifications(notifications);
      const stable = { appearance: Object.fromEntries(Object.keys(appearanceOptions).map((k) => [k, appearance[k]])), notifications };
      if (phase === "baseline-a") ctx.emit("preferences.userA.uiBaseline", stable);
      else if (phase === "baseline-b") ctx.emit("preferences.userB.uiBaseline", stable);
      else expect(stable).toEqual(receipts["preferences.userB.uiBaseline"]);
      await page.reload();
      await expect(heading()).toBeVisible();
      await assertAppearance(appearance);
      await assertNotifications(notifications);
    } else if (phase === "appearance") {
      for (const [key, options] of Object.entries(appearanceOptions)) {
        const target = config.appearance[key];
        expect(Object.hasOwn(options, String(target))).toBe(true);
        const current = (await get(appearancePath)).preferences;
        await assertAppearance(current);
        const label = options[String(target)];
        if (current[key] === target) {
          await unchangedSelection(appearancePath, key, target, label);
        } else {
          await save(appearancePath,
            () => page.getByRole("button", { name: label, exact: true }).click(),
            { [key]: target });
          await activeButton(label);
        }
      }
      // Hold the first real write while further UI edits queue. The latest
      // value for each queued field must survive; only one write may run at a
      // time. Releasing the route sends it to CF, never a fabricated success.
      const requests = [];
      let release;
      const gate = new Promise((resolve) => { release = resolve; });
      await page.route(ctx.url(appearancePath), async (route) => {
        if (route.request().method() === "PATCH") {
          requests.push(route.request().postDataJSON());
          if (requests.length === 1) await gate;
        }
        await route.continue();
      });
      try {
        const oppositeTheme = config.appearance.theme === "light" ? "dark" : "light";
        await page.getByRole("button", { name: appearanceOptions.theme[oppositeTheme], exact: true }).click();
        await expect.poll(() => requests.length).toBe(1);
        await page.getByRole("button", { name: appearanceOptions.density[config.appearance.density === "compact" ? "comfortable" : "compact"], exact: true }).click();
        await page.getByRole("button", { name: appearanceOptions.theme[config.appearance.theme], exact: true }).click();
        await page.getByRole("button", { name: appearanceOptions.density[config.appearance.density], exact: true }).click();
        expect(requests).toHaveLength(1);
        expect(requests[0].theme).toBe(oppositeTheme);
        const second = page.waitForResponse((r) => new URL(r.url()).pathname === appearancePath
          && r.request().method() === "PATCH" && r.request().postDataJSON().theme === config.appearance.theme);
        release();
        const response = await second;
        expect(response.status()).toBe(200);
        expect(projectAppearance(await response.json())).toEqual(config.appearance);
        expect(requests).toHaveLength(2);
        expect(requests[1]).toMatchObject({ theme: config.appearance.theme, density: config.appearance.density });
        await assertIdle(appearancePath, "queue-drained");
        ctx.emit("preferences.appearance.serialQueue", { writes: requests.length, latestValues: config.appearance });
      } finally { release(); await page.unroute(ctx.url(appearancePath)); }
      await page.reload();
      await expect(heading()).toBeVisible();
      await assertAppearance(config.appearance);
      const actual = (await get(appearancePath)).preferences;
      for (const [key, value] of Object.entries(config.appearance)) expect(actual[key]).toEqual(value);
      ctx.emit("preferences.appearance.reloaded", config.appearance);
    } else if (phase === "notifications") {
      const target = config.notifications;
      const capability = await get(notificationPath);
      expect(projectNotifications(capability)).not.toEqual(target);
      await assertNotifications(projectNotifications(capability));
      if (target.weekly_digest || target.delivery_channel !== "in_app") expect(capability.email_available).toBe(true);
      if (!capability.email_available) {
        await expect(checkbox("weekly_digest")).toBeDisabled();
        await expect(page.getByRole("button", { name: "Email", exact: true })).toBeDisabled();
        await expect(page.getByRole("button", { name: "Both", exact: true })).toBeDisabled();
        ctx.emit("preferences.notifications.emailUnavailable", true);
      }
      for (const key of Object.keys(notificationLabels)) {
        const control = checkbox(key);
        // setChecked on an unchanged value emits no request. A changed control
        // must receive and expose the real server-confirmed value before next.
        if (await control.isChecked() !== target[key]) {
          await save(notificationPath, () => control.setChecked(target[key]), { [key]: target[key] });
        }
        await expect(control).toBeChecked({ checked: target[key] });
      }
      const currentChannel = (await get(notificationPath)).delivery_channel;
      await activeButton(channelLabels[currentChannel]);
      const channelLabel = channelLabels[target.delivery_channel];
      if (currentChannel === target.delivery_channel) {
        await unchangedSelection(notificationPath, "delivery_channel", target.delivery_channel, channelLabel);
      } else {
        await save(notificationPath,
          () => page.getByRole("button", { name: channelLabel, exact: true }).click(),
          { delivery_channel: target.delivery_channel });
        await activeButton(channelLabel);
      }
      await page.reload();
      await expect(heading()).toBeVisible();
      await assertNotifications(target);
      expect(projectNotifications(await get(notificationPath))).toEqual(target);
      ctx.emit("preferences.notifications.reloaded", target);
    } else {
      const confirmedAppearance = (await get(appearancePath)).preferences;
      const confirmedNotificationRecord = await get(notificationPath);
      const confirmedNotifications = projectNotifications(confirmedNotificationRecord);
      await assertAppearance(confirmedAppearance);
      await assertNotifications(confirmedNotifications);
      const appearance = phase.startsWith("appearance-");
      const path = appearance ? appearancePath : notificationPath;
      const pattern = ctx.url(path);
      if (phase.endsWith("read-failure")) {
        // Abort only the actual read. No synthetic status, body or producer
        // data is supplied, and the failure never reaches persistence.
        let aborted = 0;
        await page.route(pattern, async (route) => {
          if (route.request().method() === "GET") { aborted++; await route.abort("failed"); }
          else await route.continue();
        });
        try {
          await page.reload();
          await expect(heading()).toBeVisible();
          const prefix = appearance ? "Could not initialize account preferences:" : "Could not load notification preferences:";
          await expect(page.getByRole("main").getByText(new RegExp(`^${prefix}`))).toBeVisible();
          expect(aborted).toBeGreaterThan(0);
          await ctx.screenshot(`preferences-${phase}-error`);
        } finally { await page.unroute(pattern); }
        await page.reload();
        await expect(heading()).toBeVisible();
        await assertAppearance(confirmedAppearance);
        await assertNotifications(confirmedNotifications);
        ctx.emit(`preferences.transport.${phase}`, { aborted });
      } else if (phase.endsWith("save-failure")) {
        let aborted = 0;
        await page.route(pattern, async (route) => {
          if (route.request().method() === "PATCH") { aborted++; await route.abort("failed"); }
          else await route.continue();
        });
        try {
          if (appearance) {
            const opposite = confirmedAppearance.theme === "light" ? "Dark" : "Light";
            await page.getByRole("button", { name: opposite, exact: true }).click();
          } else {
            // A rejected save rolls the checkbox back immediately. click()
            // tests that event without setChecked's conflicting postcondition.
            await checkbox("deploy_failures").click();
          }
          const prefix = appearance ? "Could not save preferences:" : "Could not save notification preferences:";
          await expect(page.getByRole("main").getByText(new RegExp(`^${prefix}`))).toBeVisible();
          expect(aborted).toBe(1);
          if (!appearance) await expect(checkbox("deploy_failures")).toBeChecked({ checked: confirmedNotifications.deploy_failures });
          await ctx.screenshot(`preferences-${phase}-error`);
        } finally { await page.unroute(pattern); }
        const beforeRetry = await get(path);
        // A transport-rejected write must leave the exact server record intact,
        // including its write timestamp, before a real user retry is attempted.
        expect(appearance ? beforeRetry.preferences : beforeRetry)
          .toEqual(appearance ? confirmedAppearance : confirmedNotificationRecord);
        await assertIdle(path, "before-retry");
        const retryKey = appearance ? "theme" : "deploy_failures";
        const retryValue = appearance ? (confirmedAppearance.theme === "light" ? "dark" : "light")
          : !confirmedNotifications.deploy_failures;
        const originalValue = appearance ? confirmedAppearance.theme : confirmedNotifications.deploy_failures;
        const trigger = (value) => appearance
          ? page.getByRole("button", { name: appearanceOptions.theme[value], exact: true }).click()
          : checkbox("deploy_failures").setChecked(value);
        await save(path, () => trigger(retryValue), { [retryKey]: retryValue });
        await save(path, () => trigger(originalValue), { [retryKey]: originalValue });
        await assertIdle(path, "after-retry");
        ctx.emit(`preferences.transport.${phase}.retry`, true);
        await page.reload();
        await expect(heading()).toBeVisible();
        await assertAppearance(confirmedAppearance);
        await assertNotifications(confirmedNotifications);
        ctx.emit(`preferences.transport.${phase}`, { aborted });
      } else if (phase === "notification-loading") {
        let release;
        const held = page.waitForRequest((r) => new URL(r.url()).pathname === path && r.method() === "GET");
        const gate = new Promise((resolve) => { release = resolve; });
        await page.route(pattern, async (route) => {
          if (route.request().method() === "GET") await gate;
          await route.continue();
        });
        try {
          await page.reload();
          await held;
          await expect(page.getByText("Loading notification preferences...", { exact: true })).toBeVisible();
          await expect(checkbox("deploy_failures")).toHaveCount(0);
          const response = page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().method() === "GET");
          release();
          expect((await response).status()).toBe(200);
          await assertNotifications(confirmedNotifications);
          await expect(page.getByText("Loading notification preferences...", { exact: true })).toHaveCount(0);
        } finally { release(); await page.unroute(pattern); }
      }
    }
    await ctx.screenshot(`preferences-${phase}`);
    ctx.emit(`preferences.browser.${phase}`, true);
  });
};
