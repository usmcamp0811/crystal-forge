/** TASK-470: one stable Add dialog, retained drafts and real scoped creates.
 * Only uniquely named workflow records are deleted. No create is mocked.
 */
const assert = require("node:assert/strict");
const { expect } = require("@playwright/test");

async function sharedCacheModalWorkflow(page, baseUrl, apiBaseUrl, screenshot, captureState) {
  const prefix = `task470-shared-${Date.now()}`;
  const key = `fixture:${Buffer.alloc(32, 7).toString("base64")}`;
  const requests = [];
  const capture = request => { if (request.url().includes("/caches")) requests.push(request); };
  page.on("request", capture);
  const api = async (method, route, data) => {
    const cookies = await page.context().cookies(apiBaseUrl);
    const csrf = cookies.find(c => c.name.includes("csrf"));
    const result = await page.evaluate(async ({ url, method, csrf, data }) => {
      const response = await fetch(url, { method, credentials: "include", headers: { "Content-Type": "application/json", ...(csrf ? { "X-CSRF-Token": csrf } : {}) }, body: data === undefined ? undefined : JSON.stringify(data) });
      return { status: response.status, text: await response.text() };
    }, { url: `${apiBaseUrl}/api/v1${route}`, method, csrf: csrf?.value, data });
    assert(result.status >= 200 && result.status < 300, `${method} ${route}: HTTP ${result.status}`);
    return result.text ? JSON.parse(result.text) : null;
  };
  const dialog = page.getByRole("dialog", { name: "Cache destination", exact: true });
  const section = name => dialog.getByRole("button", { name, exact: true }).click();
  const type = async name => {
    await section("Destination");
    await dialog.getByRole("button", { name, exact: true }).click();
    await expect(dialog.getByRole("button", { name, exact: true })).toHaveAttribute("aria-pressed", "true");
  };
  const extraCapture = async state => {
    if (captureState) await captureState(state);
    else if (screenshot) {
      const theme = await page.locator("html").getAttribute("data-theme");
      for (const value of ["dark", "light"]) {
        await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), value);
        await page.screenshot({ path: screenshot.replace(/\.png$/, `-${state}-${value}.png`), animations: "disabled" });
      }
      await page.evaluate(value => document.documentElement.setAttribute("data-theme", value), theme || "dark");
    }
  };
  const outerKeyboard = async (action, unavailable = false) => {
    const close = dialog.getByRole("button", { name: "Close", exact: true });
    await expect(close).toBeFocused();
    const primary = dialog.getByRole("button", { name: action, exact: true });
    if (unavailable) await expect(primary).toBeDisabled();
    else if (action === "Save changes") await expect(primary).toBeEnabled();
    const last = await primary.isEnabled() ? primary : dialog.getByRole("button", { name: "Cancel", exact: true });
    await expect(last).toBeEnabled();
    await page.keyboard.press("Shift+Tab");
    await expect(last).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(close).toBeFocused();
    for (let i = 0; i < 22; i++) {
      await page.keyboard.press("Tab");
      assert(await dialog.evaluate(node => node.contains(document.activeElement)), "Tab stays inside the outer cache dialog");
    }
  };
  const editSmoke = async (created, selectedEnvironment) => {
    const cachePath = `/caches/${created.id}`;
    // Exercise a disabled destination and stored but unused legacy fields. No
    // UI control may activate the former or advertise the latter as bearer auth.
    if (created.cache_type === "S3") await api("PUT", cachePath, { enabled: false });
    if (created.cache_type === "Nix") await api("PUT", cachePath, {
      attic_token: "fixture-unused-token", s3_session_token: "fixture-unused-session",
      compression: "xz", signing_key_path: "/fixture/nix-signing", max_retries: 4,
      retry_delay_seconds: 9, push_timeout_seconds: 120, parallel_uploads: 3, require_sigs: true,
    });
    const before = await api("GET", cachePath);
    await page.goto(`${baseUrl}/caches`);
    await page.getByText(created.name, { exact: true }).click();
    const panel = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
    const trigger = panel.getByRole("button", { name: "Edit cache", exact: true });
    const open = async () => { await trigger.click(); await expect(dialog).toBeVisible(); };
    const envRoute = `${apiBaseUrl}/api/v1/caches/${created.id}/environments`;
    await page.route(envRoute, async route => {
      if (route.request().method() === "GET" && await dialog.count() > 0) return route.fulfill({ status: 503, json: { error: "fixture scope read failure" } });
      await route.continue();
    });
    try {
      await open();
      await expect(dialog.getByRole("alert")).toContainText("Environment assignments could not be loaded");
      await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
      await expect(dialog.locator("footer")).toContainText("Scope not loaded");
      await expect(dialog.locator("footer")).not.toContainText("Global scope");
      await outerKeyboard("Save changes", true);
      const viewport = page.viewportSize();
      await page.setViewportSize({ width: 390, height: 844 });
      await extraCapture(`edit-${created.cache_type.toLowerCase()}-scope-error-390`);
      await page.keyboard.press("Escape");
      await expect(dialog).toBeHidden();
      await expect(trigger).toBeFocused();
      await page.setViewportSize(viewport);
    } finally { await page.unroute(envRoute); }
    await open();
    await outerKeyboard("Save changes");
    await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(created.name);
    await expect(dialog.getByRole("button", { name: "Niks3", exact: true })).toBeDisabled();
    const bodyNode = await dialog.elementHandle();
    for (const name of ["Destination", "Credentials", "Environments"]) await expect(dialog.getByRole("button", { name, exact: true })).toBeVisible();
    await extraCapture(`edit-${created.cache_type.toLowerCase()}-destination`);
    await section("Credentials");
    for (const label of ["AWS secret access key", "AWS session token (optional)", "Attic token", "Write token"]) {
      const input = dialog.getByLabel(label, { exact: true });
      if (await input.count()) await expect(input).toHaveValue("");
    }
    if (created.cache_type === "Nix") {
      await expect(dialog.getByRole("button", { name: "Add credential", exact: true })).toHaveCount(0);
      await expect(dialog.getByText("A generic bearer-token provider is not supported", { exact: false })).toBeVisible();
    }
    await extraCapture(`edit-${created.cache_type.toLowerCase()}-credentials`);
    await section("Environments");
    await expect(dialog.getByRole("button", { name: selectedEnvironment.name, exact: true })).toHaveAttribute("aria-pressed", "true");
    await extraCapture(`edit-${created.cache_type.toLowerCase()}-environments`);
    await dialog.getByRole("button", { name: selectedEnvironment.name, exact: true }).click();
    await expect(dialog.locator("footer")).toContainText("Global scope");
    assert(await dialog.evaluate((current, original) => current === original, bodyNode), "Edit sections preserve the same shared shell");
    const save = async expectedIds => {
      const mutationUrl = `${apiBaseUrl}/api/v1${cachePath}`;
      const hold = created.cache_type === "S3" && expectedIds.length === 0;
      let release;
      const gate = new Promise(resolve => { release = resolve; });
      const delay = async route => { if (route.request().method() === "PUT") await gate; await route.continue(); };
      if (hold) await page.route(mutationUrl, delay);
      let response;
      try {
        const responsePromise = page.waitForResponse(r => r.request().method() === "PUT" && r.url() === mutationUrl);
        await dialog.getByRole("button", { name: "Save changes", exact: true }).click();
        if (hold) {
          await expect(dialog.getByText("Working", { exact: true })).toBeFocused();
          await page.keyboard.press("Tab");
          await page.keyboard.press("Escape");
          await expect(dialog).toBeVisible();
          await expect(dialog.getByText("Working", { exact: true })).toBeFocused();
          await expect(dialog.getByRole("button", { name: "Close", exact: true })).toBeDisabled();
        }
        release();
        response = await responsePromise;
      } finally { release(); if (hold) await page.unroute(mutationUrl, delay); }
      assert.equal(response.status(), 200, `${created.cache_type} edit: ${await response.text()}`);
      const body = response.request().postDataJSON();
      assert.deepEqual(body.environment_ids, expectedIds);
      assert.equal(body.cache_type, null, "unchanged type is omitted, preserving exact legacy wire type");
      assert.equal(body.enabled, null, "Edit does not activate a disabled cache");
      for (const field of ["s3_access_key_id", "s3_secret_access_key", "s3_session_token", "attic_token", "niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key"]) assert.equal(body[field], null, `blank edit retains ${field}`);
      await expect(dialog).toBeHidden();
      await expect(trigger).toBeFocused();
      assert.deepEqual(await api("GET", `${cachePath}/environments`), expectedIds);
    };
    await save([]);
    const after = await api("GET", cachePath);
    for (const field of ["cache_type", "enabled", "push_to", "compression", "signing_key_path", "s3_region", "s3_profile", "s3_access_key_id", "s3_endpoint_url", "attic_cache_name", "attic_public_key", "niks3_public_keys", "niks3_write_auth_mode", "niks3_read_auth_mode", "niks3_write_token_configured", "max_retries", "retry_delay_seconds", "push_timeout_seconds", "parallel_uploads", "require_sigs"]) assert.deepEqual(after[field], before[field], `${created.cache_type} edit preserves ${field}`);
    await open();
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
    await section("Environments");
    await dialog.getByRole("button", { name: selectedEnvironment.name, exact: true }).click();
    await save([selectedEnvironment.id]);
    if (created.cache_type === "Http") {
      // Preserve the old explicit legacy-conversion capability, while proving
      // that unused stored tokens are not exposed or borrowed by the editor.
      const convert = async target => {
        const responsePromise = page.waitForResponse(r => r.request().method() === "PUT" && r.url() === `${apiBaseUrl}/api/v1${cachePath}`);
        await dialog.getByRole("button", { name: "Save changes", exact: true }).click();
        const response = await responsePromise;
        assert.equal(response.status(), 200, `explicit ${target} conversion: ${await response.text()}`);
        assert.equal(response.request().postDataJSON().cache_type, target);
        assert.deepEqual(response.request().postDataJSON().environment_ids, [selectedEnvironment.id]);
        await expect(dialog).toBeHidden();
        await expect(trigger).toBeFocused();
        assert.equal((await api("GET", cachePath)).cache_type, target);
      };
      await open();
      await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
      await type("Attic");
      await dialog.getByLabel("Attic cache name", { exact: true }).fill("fixture-converted");
      await dialog.getByLabel("Attic public key", { exact: true }).fill(key);
      await section("Credentials");
      await expect(dialog.getByLabel("Attic token", { exact: true })).toHaveValue("");
      await dialog.getByLabel("Attic token", { exact: true }).fill("fixture-explicit-conversion-token");
      await convert("Attic");
      await open();
      await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
      await type("Nix HTTPS");
      await convert("Nix");
      const converted = await api("GET", cachePath);
      assert.equal(converted.attic_public_key, key, "returning to Nix does not delete unused Attic configuration");
      console.log("TASK-470 explicit Http -> Attic -> Nix conversion and inactive-field preservation passed");
    }
    await panel.getByRole("button", { name: "Close", exact: true }).click();
    console.log(`TASK-470 shared ${created.cache_type} Edit: keyboard return, scope-error block, blank-secret retention, atomic scope clear/restore passed`);
    if (created.cache_type === "Nix") {
      // Http is an existing legacy wire type, presented as Nix HTTPS. An
      // unrelated edit must not normalize it or discard unused stored fields.
      await api("PUT", cachePath, { cache_type: "Http" });
      await editSmoke({ ...created, cache_type: "Http" }, selectedEnvironment);
    }
  };
  const shell = async () => {
    await expect(dialog).toBeVisible();
    for (const name of ["Destination", "Credentials", "Environments"]) {
      const button = dialog.getByRole("navigation").getByRole("button", { name, exact: true });
      await expect(button).toBeVisible();
      assert.equal(await button.locator("svg").count(), 1, `${name} has its design icon`);
    }
    await expect(dialog.locator("header").getByRole("button", { name: "Close", exact: true })).toBeVisible();
    await expect(dialog.locator("footer").getByRole("button", { name: "Cancel", exact: true })).toBeVisible();
    await expect(dialog.locator("footer").getByRole("button", { name: "Add cache", exact: true })).toBeVisible();
    await expect(dialog.locator("footer").getByRole("button", { name: "Add cache", exact: true }).locator("svg")).toHaveCount(1, { timeout: 60000 });
  };
  const credentialDraft = async (kind, phase, retainCapture) => {
    const outerNode = await dialog.elementHandle();
    const trigger = dialog.getByRole("button", { name: "Add credential", exact: true });
    const nested = page.getByRole("dialog", { name: "Add credential", exact: true });
    const saveDraft = nested.getByRole("button", { name: "Save credential", exact: true });
    const before = requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length;
    const captureNested = async state => {
      if (!retainCapture) return;
      if (captureState) await captureState(`nested-${phase}-${state}`);
      else if (screenshot) {
        const originalTheme = await page.locator("html").getAttribute("data-theme");
        for (const theme of ["dark", "light"]) {
          await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), theme);
          await page.screenshot({ path: screenshot.replace(/\.png$/, `-nested-${phase}-${state}-${theme}.png`), animations: "disabled" });
        }
        await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), originalTheme || "dark");
      }
    };
    await trigger.click();
    await expect(nested).toBeVisible();
    await expect(nested).toHaveAttribute("aria-modal", "true");
    await expect(nested.getByLabel("Name", { exact: true })).toBeFocused();
    await expect(saveDraft).toBeDisabled();
    // Credential confirmation creates only a local draft. A name-only draft
    // must still leave the cache's primary persistence action disabled.
    await nested.getByLabel("Name", { exact: true }).fill(`incomplete-${prefix}-${phase}`);
    await saveDraft.click();
    await expect(nested).toBeHidden();
    await expect(trigger).toBeFocused();
    await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
    await expect(dialog.getByTestId("cache-save-blocked")).toContainText(kind === "s3" ? "Enter AWS access credentials" : "Enter an Attic token");
    assert.equal(requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length, before, "incomplete nested draft does not create a cache");
    await trigger.press("Enter");
    await expect(nested.getByLabel("Name", { exact: true })).toBeFocused();
    await nested.getByLabel("Name", { exact: true }).fill(`${prefix}-${phase}-credential`);
    const typeGroup = nested.getByRole("group", { name: "Type", exact: true });
    if (kind === "s3") {
      await expect(typeGroup.getByRole("button", { name: "AWS access key", exact: true })).toHaveAttribute("aria-pressed", "true");
      await nested.getByLabel("Access key ID", { exact: true }).fill("fixture-access");
      await nested.getByLabel("Secret access key", { exact: true }).fill("fixture-secret");
      await typeGroup.getByRole("button", { name: "IAM role (IRSA)", exact: true }).click();
      await nested.getByLabel("Role ARN", { exact: true }).fill("fixture-builder-profile-reference");
      await expect(nested.getByText("this form does not assume an IAM role", { exact: false })).toBeVisible();
      await captureNested("profile-reference");
      await typeGroup.getByRole("button", { name: "AWS access key", exact: true }).click();
      await expect(nested.getByLabel("Access key ID", { exact: true })).toHaveValue("fixture-access");
      await expect(nested.getByLabel("Secret access key", { exact: true })).toHaveValue("fixture-secret");
    } else {
      await expect(typeGroup.getByRole("button", { name: "Attic token", exact: true })).toHaveAttribute("aria-pressed", "true");
      await nested.getByLabel("Token", { exact: true }).fill("fixture-attic-token");
    }
    await captureNested("desktop");
    const viewport = page.viewportSize();
    await page.setViewportSize({ width: 390, height: 844 });
    await nested.getByLabel("Name", { exact: true }).focus();
    await page.keyboard.press("Shift+Tab");
    await expect(saveDraft).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(nested.getByLabel("Name", { exact: true })).toBeFocused();
    for (let i = 0; i < 9; i++) {
      await page.keyboard.press("Tab");
      assert(await nested.evaluate(node => node.contains(document.activeElement)), "Tab stays inside the nested dialog");
    }
    await captureNested("mobile-390-keyboard");
    const bounds = await nested.boundingBox();
    assert(bounds.x >= 0 && bounds.x + bounds.width <= 390, "nested dialog fits 390px viewport");
    await page.keyboard.press("Escape");
    await expect(nested).toBeHidden();
    await expect(trigger).toBeFocused();
    assert(await dialog.evaluate((current, original) => current === original, outerNode), "Escape preserves the exact outer dialog");
    await expect(dialog.getByRole("button", { name: "Credentials", exact: true })).toHaveAttribute("aria-current", "true");
    await captureNested("escape-return-390");
    await page.setViewportSize(viewport);
    // Keyboard reopening and confirmation prove that only the nested draft was
    // canceled. The outer destination, scope and type-specific inputs survive.
    await trigger.press("Enter");
    await expect(nested.getByLabel("Name", { exact: true })).toBeFocused();
    await expect(nested.getByLabel("Name", { exact: true })).toHaveValue("");
    await nested.getByLabel("Name", { exact: true }).fill(`${prefix}-${phase}-credential`);
    if (kind === "s3") {
      await nested.getByLabel("Access key ID", { exact: true }).fill("fixture-access");
      await nested.getByLabel("Secret access key", { exact: true }).fill("draft-before-edit");
      await nested.getByLabel("Secret access key", { exact: true }).fill("fixture-secret");
    } else {
      await nested.getByLabel("Token", { exact: true }).fill("draft-before-edit");
      await nested.getByLabel("Token", { exact: true }).fill("fixture-attic-token");
    }
    await saveDraft.click();
    await expect(nested).toBeHidden();
    await expect(trigger).toBeFocused();
    assert(await dialog.evaluate((current, original) => current === original, outerNode), "draft confirmation preserves the exact outer dialog");
    await expect(dialog.getByLabel("Credential", { exact: true }).locator("option:checked")).toContainText(`${prefix}-${phase}-credential`);
    await expect(dialog.getByLabel(kind === "s3" ? "AWS secret access key" : "Attic token", { exact: true })).toHaveValue(kind === "s3" ? "fixture-secret" : "fixture-attic-token");
    assert.equal(requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length, before, "confirming a credential draft does not persist a cache or credential library");
    console.log(`TASK-470 nested ${phase} labels, focus containment, Escape return and edited draft passed`);
  };
  const shots = async kind => {
    if (!screenshot && !captureState) return;
    const viewport = page.viewportSize();
    if (captureState) {
      // Register evidence with the authoritative harness so the VM driver
      // exports every section and 390px state after the browser exits.
      for (const name of ["Destination", "Credentials", "Environments"]) {
        await section(name);
        await captureState(`shared-${kind}-${name.toLowerCase()}`);
      }
      await page.setViewportSize({ width: 390, height: 844 });
      await section("Destination");
      await captureState(`shared-${kind}-mobile-390`);
      const bounds = await dialog.boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= 390, "narrow dialog fits the viewport");
      assert(await dialog.evaluate(node => node.scrollWidth <= node.clientWidth), "narrow shell does not overflow horizontally");
      await page.setViewportSize(viewport);
      await section("Destination");
      return;
    }
    const originalTheme = await page.locator("html").getAttribute("data-theme");
    for (const theme of ["dark", "light"]) {
      await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), theme);
      for (const name of ["Destination", "Credentials", "Environments"]) {
        await section(name);
        await page.screenshot({ path: screenshot.replace(/\.png$/, `-${kind}-${name.toLowerCase()}-${theme}.png`), fullPage: true, animations: "disabled" });
      }
      await page.setViewportSize({ width: 390, height: 844 });
      await section("Destination");
      await page.screenshot({ path: screenshot.replace(/\.png$/, `-${kind}-mobile-${theme}.png`), fullPage: true, animations: "disabled" });
      const bounds = await dialog.boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= 390, "narrow dialog fits the viewport");
      assert(await dialog.evaluate(node => node.scrollWidth <= node.clientWidth), "narrow shell does not overflow horizontally");
      await page.setViewportSize(viewport);
    }
    await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), originalTheme || "dark");
    await section("Destination");
  };
  try {
    await page.evaluate(() => localStorage.setItem("cf.coach.ui.v2", JSON.stringify({ panel: "dismissed", track: "setup" })));
    await page.goto(`${baseUrl}/caches`);
    const envBody = await api("GET", "/environments");
    const env = (Array.isArray(envBody) ? envBody : envBody.environments)[0];
    assert(env, "fixture contains an environment");
    const addTrigger = page.getByRole("button", { name: "Add cache", exact: true });
    await addTrigger.click();
    await outerKeyboard("Add cache");
    const addViewport = page.viewportSize();
    await page.setViewportSize({ width: 390, height: 844 });
    const reasonBounds = await dialog.getByTestId("cache-save-blocked").boundingBox();
    assert(reasonBounds && reasonBounds.width >= 300, "blocked reason uses the full mobile footer width");
    await extraCapture("outer-add-keyboard-390");
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(addTrigger).toBeFocused();
    await page.setViewportSize(addViewport);
    await addTrigger.press("Enter");
    await expect(dialog.getByRole("button", { name: "Close", exact: true })).toBeFocused();
    await shell();
    await expect(dialog.getByRole("heading", { name: "Add cache destination", exact: true })).toBeVisible();
    const node = await dialog.elementHandle();
    await dialog.getByLabel("Name", { exact: true }).fill(prefix);
    await type("Nix HTTPS");
    await dialog.getByLabel("URL", { exact: true }).fill("https://read.example.com");
    await section("Environments");
    await dialog.getByRole("button", { name: env.name, exact: true }).click();
    await type("Attic");
    await expect(dialog.getByLabel("Attic server URL", { exact: true })).toHaveValue("https://read.example.com");
    await dialog.getByLabel("Attic server URL", { exact: true }).fill("https://attic.example.com");
    await dialog.getByLabel("Attic cache name", { exact: true }).fill("fixture-attic");
    await dialog.getByLabel("Attic public key", { exact: true }).fill(key);
    await section("Credentials");
    await credentialDraft("attic", "attic-retained", false);
    await type("Niks3");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://read.example.com");
    await dialog.getByLabel("Write / API URL", { exact: true }).fill("https://write.example.com");
    await dialog.getByLabel("Signing public keys", { exact: true }).fill(key);
    await section("Credentials");
    await dialog.getByLabel("Write token", { exact: true }).fill("fixture-niks3-token");
    await type("S3-compatible");
    for (const [label, value] of [["Destination URL", "s3://fixture-bucket/prefix"], ["S3 region", "us-east-1"], ["S3 endpoint URL", "https://s3.example.com"]]) await dialog.getByLabel(label, { exact: true }).fill(value);
    await dialog.getByLabel("Signing key path (optional)", { exact: true }).fill("/fixture/signing-key");
    await dialog.getByLabel("Compression (optional)", { exact: true }).selectOption("zstd");
    await section("Credentials");
    await credentialDraft("s3", "s3", true);
    for (const [label, value] of [["AWS session token (optional)", "fixture-session"], ["S3 profile (optional)", "fixture-profile"]]) await dialog.getByLabel(label, { exact: true }).fill(value);
    for (const [name, badge] of [["Attic", "Attic"], ["Niks3", "Niks3"], ["Nix HTTPS", "Nix HTTPS"], ["S3-compatible", "S3"]]) {
      await type(name);
      assert(await dialog.evaluate((current, original) => current === original, node), `same actual dialog object after ${name}`);
      await shell();
      await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(prefix);
      await expect(dialog.locator("header")).toContainText(badge);
      await expect(dialog.locator("footer")).toContainText("1 selected");
      if (name === "Attic") {
        await expect(dialog.getByLabel("Attic server URL", { exact: true })).toHaveValue("https://attic.example.com");
        await expect(dialog.getByLabel("Attic cache name", { exact: true })).toHaveValue("fixture-attic");
        await expect(dialog.getByLabel("Attic public key", { exact: true })).toHaveValue(key);
        await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveCount(0);
        await section("Credentials");
        await expect(dialog.getByLabel("Attic token", { exact: true })).toHaveValue("fixture-attic-token");
        await expect(dialog.getByLabel("AWS access key ID", { exact: true })).toHaveCount(0);
      } else if (name === "Niks3") {
        await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue("https://read.example.com");
        await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveValue("https://write.example.com");
        await expect(dialog.getByLabel("Signing public keys", { exact: true })).toHaveValue(key);
        await expect(dialog.getByLabel("S3 region", { exact: true })).toHaveCount(0);
        await section("Credentials");
        await expect(dialog.getByLabel("Write token", { exact: true })).toHaveValue("fixture-niks3-token");
      } else if (name === "S3-compatible") {
        await expect(dialog.getByLabel("Destination URL", { exact: true })).toHaveValue("s3://fixture-bucket/prefix");
        await expect(dialog.getByLabel("S3 region", { exact: true })).toHaveValue("us-east-1");
        await expect(dialog.getByLabel("S3 endpoint URL", { exact: true })).toHaveValue("https://s3.example.com");
        await expect(dialog.getByLabel("Signing key path (optional)", { exact: true })).toHaveValue("/fixture/signing-key");
        await expect(dialog.getByLabel("Compression (optional)", { exact: true })).toHaveValue("zstd");
        await section("Credentials");
        await expect(dialog.getByLabel("AWS access key ID", { exact: true })).toHaveValue("fixture-access");
        await expect(dialog.getByLabel("AWS secret access key", { exact: true })).toHaveValue("fixture-secret");
        await expect(dialog.getByLabel("AWS session token (optional)", { exact: true })).toHaveValue("fixture-session");
        await expect(dialog.getByLabel("S3 profile (optional)", { exact: true })).toHaveValue("fixture-profile");
      } else {
        await expect(dialog.getByLabel("URL", { exact: true })).toHaveValue("https://read.example.com");
        await expect(dialog.getByLabel("Attic cache name", { exact: true })).toHaveCount(0);
      }
      await shots(badge.toLowerCase().replace(/ /g, "-"));
    }
    await type("Niks3");
    await section("Credentials");
    await dialog.getByLabel("Write authentication").selectOption("mtls");
    await dialog.getByLabel("Read authentication").selectOption("mtls");
    for (const plane of ["Write", "Read"]) {
      await dialog.getByLabel(`${plane} client certificate`, { exact: true }).fill(`draft-${plane}-certificate`);
      await dialog.getByLabel(`${plane} private key`, { exact: true }).fill(`draft-${plane}-private-key`);
      await dialog.getByLabel(`${plane} CA certificate (optional)`, { exact: true }).fill(`draft-${plane}-CA`);
    }
    await type("Attic");
    await type("Niks3");
    assert(await dialog.evaluate((current, original) => current === original, node), "mTLS switching retains the actual dialog object");
    await section("Credentials");
    for (const plane of ["Write", "Read"]) {
      await expect(dialog.getByLabel(`${plane} client certificate`, { exact: true })).toHaveValue(`draft-${plane}-certificate`);
      await expect(dialog.getByLabel(`${plane} private key`, { exact: true })).toHaveValue(`draft-${plane}-private-key`);
      await expect(dialog.getByLabel(`${plane} CA certificate (optional)`, { exact: true })).toHaveValue(`draft-${plane}-CA`);
    }
    await dialog.getByLabel("Write authentication").selectOption("token");
    await dialog.getByLabel("Read authentication").selectOption("none");
    await dialog.getByLabel("Write token").fill("fixture-niks3-token");
    await type("S3-compatible");
    await section("Destination");
    await dialog.getByLabel("Name", { exact: true }).fill("");
    const before = requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length;
    await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
    await expect(dialog.getByTestId("cache-save-blocked")).toBeVisible();
    await expect(dialog.getByTestId("cache-save-blocked")).toContainText("Enter a cache name");
    assert.equal(requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length, before, "invalid draft never reaches create");
    await dialog.getByLabel("Name", { exact: true }).fill(`${prefix}-S3`);
    for (const [button, apiType] of [["S3-compatible", "S3"], ["Attic", "Attic"], ["Nix HTTPS", "Nix"], ["Niks3", "Niks3"]]) {
      // Reuse the retained-draft dialog for the first real create. Subsequent
      // records start in fresh dialogs, proving actual default-to-create paths.
      if (apiType !== "S3") {
        await page.getByRole("button", { name: "Add cache", exact: true }).click();
        await type(button);
        await dialog.getByLabel("Name", { exact: true }).fill(`${prefix}-${apiType}`);
        await dialog.getByLabel(apiType === "Niks3" ? "Read / substituter URL" : apiType === "Attic" ? "Attic server URL" : "URL", { exact: true }).fill("https://read.example.com");
        if (apiType === "Attic") {
          await dialog.getByLabel("Attic cache name", { exact: true }).fill("fixture-attic");
          await dialog.getByLabel("Attic public key", { exact: true }).fill(key);
          await section("Credentials");
          await credentialDraft("attic", "attic", true);
        } else if (apiType === "Niks3") {
          await dialog.getByLabel("Write / API URL", { exact: true }).fill("https://write.example.com");
          await dialog.getByLabel("Signing public keys", { exact: true }).fill(key);
          await section("Credentials");
          await dialog.getByLabel("Write token", { exact: true }).fill("fixture-niks3-token");
        }
        await section("Environments");
        await dialog.getByRole("button", { name: env.name, exact: true }).click();
      }
      const responsePromise = page.waitForResponse(r => r.request().method() === "POST" && /\/api\/v1\/caches$/.test(r.url()));
      await dialog.getByRole("button", { name: "Add cache", exact: true }).click();
      const response = await responsePromise;
      assert.equal(response.status(), 201, `${apiType} real create: ${await response.text()}`);
      const created = await response.json();
      await expect(dialog).toBeHidden();
      assert.equal(created.cache_type, apiType);
      if (apiType === "Attic") assert.equal(response.request().postDataJSON().attic_token, "fixture-attic-token", "real Attic create uses the confirmed nested token draft");
      if (apiType === "S3") {
        assert.equal(response.request().postDataJSON().s3_access_key_id, "fixture-access");
        assert.equal(response.request().postDataJSON().s3_secret_access_key, "fixture-secret", "real S3 create uses the confirmed nested access-key draft");
        assert.equal(created.s3_region, "us-east-1");
        assert.equal(created.s3_profile, "fixture-profile");
        assert.equal(created.s3_endpoint_url, "https://s3.example.com");
        assert.equal(created.compression, "zstd");
        assert.equal(created.signing_key_path, "/fixture/signing-key");
      }
      assert.deepEqual(response.request().postDataJSON().environment_ids, [env.id]);
      assert.deepEqual(await api("GET", `/caches/${created.id}/environments`), [env.id]);
      console.log(`TASK-470 shared dialog real ${apiType} create passed (HTTP 201, atomic scope)`);
      await editSmoke(created, env);
    }
    assert(!requests.some(r => r.method() === "PUT" && /\/environments$/.test(r.url())), "no second environment PUT for any type");
    const environmentsRoute = `${apiBaseUrl}/api/v1/environments`;
    let release;
    const gate = new Promise(resolve => { release = resolve; });
    await page.route(environmentsRoute, async route => {
      await gate;
      await route.fulfill({ status: 503, json: { error: "fixture environments unavailable" } });
    });
    try {
      await page.getByRole("button", { name: "Add cache", exact: true }).click();
      await section("Environments");
      await expect(dialog.getByText("Loading environments…", { exact: true })).toBeVisible();
      await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
      await expect(dialog.locator("footer")).toContainText("Scope not loaded");
      await expect(dialog.locator("footer")).not.toContainText("Global scope");
      if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-environment-loading.png"), animations: "disabled" });
      if (captureState) await captureState("shared-environment-loading");
      release();
      await expect(dialog.getByRole("alert")).toContainText("Could not load environments");
      await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
      if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-environment-error.png"), animations: "disabled" });
      if (captureState) await captureState("shared-environment-error");
      await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    } finally { release(); await page.unroute(environmentsRoute); }
    console.log("TASK-470 shared dialog identity, four-type draft retention, validation and 4 real creates passed");
  } finally {
    // Recover records even if an assertion fails immediately after creation.
    for (const cache of await api("GET", "/caches")) if (cache.name.startsWith(prefix)) await api("DELETE", `/caches/${cache.id}`);
    page.off("request", capture);
  }
}

module.exports = { sharedCacheModalWorkflow };
