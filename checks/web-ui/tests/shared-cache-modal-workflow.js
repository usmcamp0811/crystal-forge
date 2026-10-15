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
  const identity = async (plane, action) => {
    await section(plane === "Write" ? "Write / API" : "Read / Pull");
    await dialog.getByRole("button", { name: /^(Add credential|Replace|Edit replacement)$/ }).click();
    const nested = page.getByRole("dialog", { name: `${plane} credential`, exact: true });
    await expect(nested).toBeVisible();
    await action(nested);
    await nested.getByRole("button", { name: "Use for this cache", exact: true }).click();
    await expect(nested).toBeHidden();
  };
  const localCredential = async action => {
    await dialog.getByRole("button", { name: /^(Add|Edit|Replace) credential$/ }).click();
    const nested = page.getByRole("dialog", { name: "Add credential", exact: true });
    await action(nested);
    await nested.getByRole("button", { name: "Save credential", exact: true }).click();
    await expect(nested).toBeHidden();
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
    // Defensive metadata tests alter only redacted GET presentation. Stored-ID
    // probes below continue to the real server; no Test response is mocked.
    const flags = ["attic_token_configured", "s3_credentials_configured", "s3_session_token_configured",
      "niks3_write_token_configured", "niks3_write_mtls_configured", "niks3_read_mtls_configured",
      "http_basic_auth_configured", "legacy_query_credentials_configured"];
    const listRoute = `${apiBaseUrl}/api/v1/caches`;
    const staleList = async route => {
      if (route.request().method() !== "GET") return route.continue();
      const response = await route.fetch();
      const values = await response.json();
      await route.fulfill({ response, json: values.map(value => value.id === created.id
        ? { ...value, ...Object.fromEntries(flags.map(flag => [flag, false])), push_to: value.cache_type === "S3" ? "s3://stale-list-bucket" : "https://stale-list.example" }
        : value) });
    };
    let releaseLoad;
    const loadGate = new Promise(resolve => { releaseLoad = resolve; });
    let firstLoad = true;
    const freshRoute = `${apiBaseUrl}/api/v1${cachePath}`;
    const delayThenFail = async route => {
      if (route.request().method() === "GET" && firstLoad) {
        firstLoad = false;
        await loadGate;
        return route.fulfill({ status: 503, json: { error: "fixture destination unavailable" } });
      }
      await route.continue();
    };
    await page.route(listRoute, staleList);
    await page.route(freshRoute, delayThenFail);
    const editRequestsBeforeLoad = requests.filter(r => r.method() !== "GET" && r.url().includes(cachePath)).length;
    try {
      await page.goto(`${baseUrl}/caches`);
      await page.getByText(created.name, { exact: true }).click();
      const parent = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
      const opener = parent.getByRole("button", { name: "Edit cache", exact: true });
      await opener.click();
      await expect(dialog.getByRole("status")).toHaveText("Loading destination…");
      await expect(dialog.getByLabel("Name", { exact: true })).toHaveCount(0);
      await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toHaveCount(0);
      await expect(dialog.getByRole("button", { name: "Test connection", exact: true })).toHaveCount(0);
      releaseLoad();
      await expect(dialog.getByRole("alert")).toContainText("Destination could not be loaded");
      await expect(dialog.getByLabel("Name", { exact: true })).toHaveCount(0);
      const freshResponse = page.waitForResponse(r => r.url() === freshRoute && r.request().method() === "GET" && r.status() === 200);
      await dialog.getByRole("button", { name: "Retry loading destination", exact: true }).click();
      await freshResponse;
      await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
      await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(before.name);
      await expect(dialog.getByLabel(created.cache_type === "S3" ? "Destination URL" : created.cache_type === "Attic" ? "Attic server URL" : created.cache_type === "Niks3" ? "Read / substituter URL" : "URL", { exact: true })).toHaveValue(before.push_to);
      await section("Credentials");
      if (created.cache_type === "Attic" || created.cache_type === "S3") await expect(dialog.getByLabel("Credential", { exact: true })).toHaveValue("__current__");
      if (created.cache_type === "Niks3") await expect(dialog.getByLabel("Write credential", { exact: true })).toHaveValue("__current__");
      assert.equal(requests.filter(r => r.method() !== "GET" && r.url().includes(cachePath)).length, editRequestsBeforeLoad, "loading/error/retry never probe or save stale list data");
      await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(opener).toBeFocused();
      await parent.getByRole("button", { name: "Close", exact: true }).click();
    } finally { releaseLoad(); await page.unroute(freshRoute, delayThenFail); await page.unroute(listRoute, staleList); }
    const missingFlags = async route => {
      if (route.request().method() !== "GET") return route.continue();
      const response = await route.fetch();
      const value = await response.json();
      for (const flag of flags) delete value[flag];
      await route.fulfill({ response, json: value });
    };
    await page.route(freshRoute, missingFlags);
    try {
      await page.goto(`${baseUrl}/caches`);
      await page.getByText(created.name, { exact: true }).click();
      const parent = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
      const freshResponse = page.waitForResponse(r => r.url() === freshRoute && r.request().method() === "GET");
      await parent.getByRole("button", { name: "Edit cache", exact: true }).click();
      assert.equal((await freshResponse).status(), 200, "Edit fetches its baseline by ID");
      await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(before.name);
      await section("Credentials");
      await expect(dialog.getByRole("button", { name: "Test connection", exact: true })).toBeEnabled();
      await expect(dialog).toContainText("Stored credential status unavailable");
      const testUrl = `${freshRoute}/test-credentials`;
      const probe = page.waitForResponse(r => r.url() === testUrl && r.request().method() === "POST");
      await dialog.getByRole("button", { name: "Test connection", exact: true }).click();
      const response = await probe;
      // These discovery/draft fixtures use example.com endpoints, not the
      // native provider fixture. Prove server authority and secret omission;
      // the sibling native workflow requires real successful authentication.
      assert([200, 400].includes(response.status()), "missing metadata reaches the authoritative stored-ID API");
      if (response.status() === 400) assert.equal((await response.json()).error, "invalid_cache_test_config", "example endpoint rejection is an explicit server result");
      const patch = response.request().postDataJSON();
      for (const field of ["attic_token", "s3_access_key_id", "s3_secret_access_key", "s3_session_token", "niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key"]) assert(!Object.hasOwn(patch, field), `retained Test omits ${field}`);
      if (created.cache_type !== "Niks3") assert(patch.push_to == null, "unchanged fresh public URL remains omitted");
      if (created.cache_type === "Attic" || created.cache_type === "S3") {
        await dialog.getByRole("button", { name: "Add credential", exact: true }).click();
        const nested = page.getByRole("dialog", { name: "Add credential", exact: true });
        await nested.getByLabel("Name", { exact: true }).fill("incomplete replacement");
        await nested.getByRole("button", { name: "Save credential", exact: true }).click();
        const count = requests.filter(r => r.url() === testUrl).length;
        await dialog.getByRole("button", { name: "Test connection", exact: true }).click();
        await expect(dialog.getByRole("alert")).toContainText("Test not run");
        assert.equal(requests.filter(r => r.url() === testUrl).length, count, "explicit incomplete replacement never borrows retained identity");
        await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
      }
      await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
      await parent.getByRole("button", { name: "Close", exact: true }).click();
      console.log(`TASK-470 ${created.cache_type}: fresh GET loading/error/retry; missing flags real ID Test HTTP ${response.status()}; secret omission passed`);
    } finally { await page.unroute(freshRoute, missingFlags); }
    await page.goto(`${baseUrl}/caches`);
    await page.getByText(created.name, { exact: true }).click();
    const panel = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
    const trigger = panel.getByRole("button", { name: "Edit cache", exact: true });
    const open = async () => {
      const freshResponse = page.waitForResponse(r => r.url() === `${apiBaseUrl}/api/v1${cachePath}` && r.request().method() === "GET");
      await trigger.click();
      assert.equal((await freshResponse).status(), 200, "Edit bootstrap GET succeeds");
      await expect(dialog.getByLabel("Name", { exact: true })).toBeVisible();
    };
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
    if (created.cache_type === "Attic" || created.cache_type === "S3") {
      const configuredFlag = created.cache_type === "Attic" ? "attic_token_configured" : "s3_credentials_configured";
      assert.equal(before[configuredFlag], true, "normal configured-true path must exercise empty replacement confirmation");
      const selector = dialog.getByLabel("Credential", { exact: true });
      await expect(selector).toHaveValue("__current__");
      const testUrl = `${apiBaseUrl}/api/v1${cachePath}/test-credentials`;
      const puts = () => requests.filter(r => r.url() === `${apiBaseUrl}/api/v1${cachePath}` && r.method() === "PUT").length;
      const probes = () => requests.filter(r => r.url() === testUrl && r.method() === "POST").length;
      for (const variant of created.cache_type === "S3" ? ["empty-key", "profile-only-role"] : ["empty-token"]) {
        const name = `incomplete-${variant}`;
        const putsBefore = puts();
        const probesBefore = probes();
        await dialog.getByRole("button", { name: "Replace credential", exact: true }).click();
        const nested = page.getByRole("dialog", { name: "Add credential", exact: true });
        await nested.getByLabel("Name", { exact: true }).fill(name);
        if (variant === "profile-only-role") {
          await nested.getByRole("group", { name: "Type", exact: true }).getByRole("button", { name: "IAM role (IRSA)", exact: true }).click();
          await nested.getByLabel("Role ARN", { exact: true }).fill("fixture-role-profile");
        } else await expect(nested.getByLabel(created.cache_type === "Attic" ? "Token" : "Secret access key", { exact: true })).toHaveValue("");
        await nested.getByRole("button", { name: "Save credential", exact: true }).click();
        await expect(nested).toBeHidden();
        await expect(selector.locator("option:checked")).toContainText(name);
        assert(!["", "__current__", "__new__"].includes(await selector.inputValue()), "confirmation preserves the explicit local draft ID");
        await expect(dialog.getByRole("button", { name: "Edit credential", exact: true })).toBeVisible();
        await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
        await dialog.getByRole("button", { name: "Test connection", exact: true }).click();
        await expect(dialog.getByRole("alert")).toContainText(created.cache_type === "Attic" ? "Test not run: Enter an Attic token" : "Test not run: Enter AWS access credentials");
        assert.equal(probes(), probesBefore, "configured credentials cannot complete an explicit empty replacement Test");
        assert.equal(puts(), putsBefore, "confirming an incomplete replacement never saves or silently retains it");
        // Only an operator selection restores retention. Restore the separate
        // public profile draft too, so the remaining unrelated-edit assertions
        // continue to check the original configuration rather than a role edit.
        await selector.selectOption("__current__");
        if (variant === "profile-only-role") await dialog.getByLabel("S3 profile (optional)", { exact: true }).fill(before.s3_profile || "");
        await expect(selector).toHaveValue("__current__");
        await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
        const responsePromise = page.waitForResponse(r => r.url() === testUrl && r.request().method() === "POST");
        await dialog.getByRole("button", { name: "Test connection", exact: true }).click();
        const response = await responsePromise;
        assert([200, 400].includes(response.status()), "explicit retention reaches the real stored-ID API");
        if (response.status() === 400) assert.equal((await response.json()).error, "invalid_cache_test_config", "example endpoint rejection remains server-authoritative");
        const patch = response.request().postDataJSON();
        for (const field of ["attic_token", "s3_access_key_id", "s3_secret_access_key", "s3_session_token"]) assert(!Object.hasOwn(patch, field), `explicit retention omits ${field}`);
        assert.equal(probes(), probesBefore + 1, "retention occurs only after selecting Current");
        assert.equal(puts(), putsBefore, "retained Test is not Save");
        await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
      }
      console.log(`TASK-470 ${created.cache_type}: configured-true empty replacements stay selected; Test/Save blocked; explicit Current permits secret-free ID POST`);
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
      for (const field of ["s3_access_key_id", "s3_secret_access_key", "s3_session_token", "attic_token", "niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key"]) assert.equal(body[field], undefined, `blank edit omits and retains ${field}`);
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
      await expect(dialog.getByLabel("Attic token", { exact: true })).toHaveCount(0);
      await localCredential(async nested => {
        await nested.getByLabel("Name", { exact: true }).fill("explicit conversion");
        await nested.getByLabel("Token", { exact: true }).fill("fixture-explicit-conversion-token");
      });
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
    const names = await dialog.getByRole("button", { name: "Write / API", exact: true }).count()
      ? ["Destination", "Write / API", "Read / Pull", "Trust", "Advanced"]
      : ["Destination", "Credentials", "Environments"];
    for (const name of names) {
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
    const trigger = dialog.getByRole("button", { name: /^(Add|Edit) credential$/ });
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
    await expect(nested.getByLabel("Name", { exact: true })).toHaveValue(`incomplete-${prefix}-${phase}`);
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
    await expect(dialog.getByLabel(kind === "s3" ? "AWS secret access key" : "Attic token", { exact: true })).toHaveCount(0);
    assert.equal(requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length, before, "confirming a credential draft does not persist a cache or credential library");
    console.log(`TASK-470 nested ${phase} labels, focus containment, Escape return and edited draft passed`);
  };
  const shots = async kind => {
    if (!screenshot && !captureState) return;
    const viewport = page.viewportSize();
    const sections = kind === "niks3" ? ["Destination", "Write / API", "Read / Pull", "Trust", "Advanced"] : ["Destination", "Credentials", "Environments"];
    if (captureState) {
      // Register evidence with the authoritative harness so the VM driver
      // exports every section and 390px state after the browser exits.
      for (const name of sections) {
        await section(name);
        await captureState(`shared-${kind}-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`);
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
      for (const name of sections) {
        await section(name);
        await page.screenshot({ path: screenshot.replace(/\.png$/, `-${kind}-${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}-${theme}.png`), fullPage: true, animations: "disabled" });
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
    await type("S3-compatible");
    for (const [label, value] of [["Destination URL", "s3://fixture-bucket/prefix"], ["S3 region", "us-east-1"], ["S3 endpoint URL", "https://s3.example.com"]]) await dialog.getByLabel(label, { exact: true }).fill(value);
    await dialog.getByLabel("Signing key path (optional)", { exact: true }).fill("/fixture/signing-key");
    await dialog.getByLabel("Compression (optional)", { exact: true }).selectOption("zstd");
    await section("Credentials");
    await credentialDraft("s3", "s3", true);
    await localCredential(nested => nested.getByLabel("AWS session token (optional)", { exact: true }).fill("fixture-session"));
    await dialog.getByLabel("S3 profile (optional)", { exact: true }).fill("fixture-profile");
    for (const [name, badge] of [["Attic", "Attic"], ["Nix HTTPS", "Nix HTTPS"], ["S3-compatible", "S3"]]) {
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
        await localCredential(nested => expect(nested.getByLabel("Token", { exact: true })).toHaveValue("fixture-attic-token"));
        await expect(dialog.getByLabel("AWS access key ID", { exact: true })).toHaveCount(0);
      } else if (name === "S3-compatible") {
        await expect(dialog.getByLabel("Destination URL", { exact: true })).toHaveValue("s3://fixture-bucket/prefix");
        await expect(dialog.getByLabel("S3 region", { exact: true })).toHaveValue("us-east-1");
        await expect(dialog.getByLabel("S3 endpoint URL", { exact: true })).toHaveValue("https://s3.example.com");
        await expect(dialog.getByLabel("Signing key path (optional)", { exact: true })).toHaveValue("/fixture/signing-key");
        await expect(dialog.getByLabel("Compression (optional)", { exact: true })).toHaveValue("zstd");
        await section("Credentials");
        await localCredential(async nested => {
          await expect(nested.getByLabel("Access key ID", { exact: true })).toHaveValue("fixture-access");
          await expect(nested.getByLabel("Secret access key", { exact: true })).toHaveValue("fixture-secret");
          await expect(nested.getByLabel("AWS session token (optional)", { exact: true })).toHaveValue("fixture-session");
        });
        await expect(dialog.getByLabel("S3 profile (optional)", { exact: true })).toHaveValue("fixture-profile");
      } else {
        await expect(dialog.getByLabel("URL", { exact: true })).toHaveValue("https://read.example.com");
        await expect(dialog.getByLabel("Attic cache name", { exact: true })).toHaveCount(0);
      }
      await shots(badge.toLowerCase().replace(/ /g, "-"));
    }
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
        if (apiType === "Niks3") {
          await section("Read / Pull");
          await dialog.getByRole("group", { name: "Read authentication", exact: true }).getByRole("button", { name: "Public", exact: true }).click();
        }
        await dialog.getByLabel(apiType === "Niks3" ? "Read / substituter URL" : apiType === "Attic" ? "Attic server URL" : "URL", { exact: true }).fill("https://read.example.com");
        if (apiType === "Attic") {
          await dialog.getByLabel("Attic cache name", { exact: true }).fill("fixture-attic");
          await dialog.getByLabel("Attic public key", { exact: true }).fill(key);
          await section("Credentials");
          await credentialDraft("attic", "attic", true);
        } else if (apiType === "Niks3") {
          await section("Write / API");
          await dialog.getByRole("group", { name: "Write authentication", exact: true }).getByRole("button", { name: "API token", exact: true }).click();
          await dialog.getByLabel("Write / API URL", { exact: true }).fill("https://write.example.com");
          await identity("Write", nested => nested.getByLabel("API token", { exact: true }).fill("fixture-niks3-token"));
          await section("Trust");
          await dialog.getByLabel("Signing public key 1", { exact: true }).fill(key);
          await shots("niks3");
        }
        await section(apiType === "Niks3" ? "Destination" : "Environments");
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
      // Dedicated Niks3 Edit/scoped-probe coverage belongs to the Niks3 workflow.
      if (apiType !== "Niks3") await editSmoke(created, env);
      if (apiType === "Attic") await atticProbeDiagnosticWorkflow(page, baseUrl, apiBaseUrl, created.id, captureState);
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
    console.log("TASK-470 generic dialog identity/draft retention, dedicated Niks3 sections, validation and 4 real creates passed");
  } finally {
    // Recover records even if an assertion fails immediately after creation.
    for (const cache of await api("GET", "/caches")) if (cache.name.startsWith(prefix)) await api("DELETE", `/caches/${cache.id}`);
    page.off("request", capture);
  }
}

/** First performs a real non-mutating stored-ID Test, then exercises explicitly
 * mocked renderer/client-decoder outcomes. No cache configuration is saved.
 * Native provider authentication remains covered by the sibling real workflow.
 */
async function atticProbeDiagnosticWorkflow(page, baseUrl, apiBaseUrl, id, captureState) {
  const url = `${apiBaseUrl}/api/v1/caches/${id}`;
  const probeUrl = `${url}/test-credentials`;
  const read = async () => {
    const result = await page.evaluate(async url => {
      const response = await fetch(url, { credentials: "include" });
      return { status: response.status, value: await response.json() };
    }, url);
    assert.equal(result.status, 200, "read-only Attic diagnostic baseline GET");
    assert.equal(result.value.cache_type, "Attic", "diagnostic requires an existing Attic destination");
    return result.value;
  };
  const before = await read();
  const mutations = [];
  const observe = request => {
    if (request.url().startsWith(`${apiBaseUrl}/api/v1/caches`) &&
      ["POST", "PUT", "PATCH", "DELETE"].includes(request.method()) &&
      request.url() !== probeUrl) mutations.push(request);
  };
  page.on("request", observe);
  const dialog = page.getByRole("dialog", { name: "Cache destination", exact: true });
  let parent;
  const forbidden = "fixture-forbidden-upstream-prose";
  const forbiddenUrl = "https://fixture-forbidden.invalid/?token=fixture-forbidden-token";
  const evidence = (stage, ok = false, privateToken = null) => ({
    ok, status_code: ok ? 200 : stage === "authentication" ? 401 : stage === "cache_not_found" ? 404 : null,
    message: forbidden, tested_url: forbiddenUrl, probe_kind: "attic_cache_config", stage,
    cache_access_valid: ok ? true : ["target_policy", "dns", "transport"].includes(stage) ? null : false,
    token_auth_valid: privateToken, write_auth_valid: null,
  });
  const constants = {
    target_policy: "Target blocked. Check the HTTPS server URL and cache name; ask an administrator to review target policy. Write authorization: Untested.",
    dns: "Cannot resolve the Attic server. Check DNS and the server URL. Write authorization: Untested.",
    transport: "Cannot connect to the Attic server with verified TLS. Check connectivity and certificate trust. Write authorization: Untested.",
    authentication: "Cache access denied. Check the token and its cache read permissions; cache existence is unresolved. Write authorization: Untested.",
    cache_not_found: "Cache not found. Check the configured cache name. Write authorization: Untested.",
    response: "Unexpected Attic cache-config response. Check the server URL and proxy path. Write authorization: Untested.",
    complete: "Cache access verified. Write authorization: Untested.",
    unverified: "Attic cache access was not verified. Check the server URL and cache name. Write authorization: Untested.",
  };
  const click = async () => {
    const response = page.waitForResponse(r => r.url() === probeUrl && r.request().method() === "POST");
    await dialog.getByRole("button", { name: "Test connection", exact: true }).click();
    return response;
  };
  const result = dialog.getByTestId("attic-test-result");
  try {
    await page.goto(`${baseUrl}/caches`);
    await page.getByText(before.name, { exact: true }).click();
    parent = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
    const fresh = page.waitForResponse(r => r.url() === url && r.request().method() === "GET");
    await parent.getByRole("button", { name: "Edit cache", exact: true }).click();
    assert.equal((await fresh).status(), 200, "diagnostic Edit uses fresh GET");
    await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(before.name);
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Test connection", exact: true })).toBeEnabled();
    // No probe interception is installed for this request.
    const real = await click();
    const actual = await real.json();
    assert([200, 400].includes(real.status()), "real Attic stored-ID Test status");
    assert.equal(actual.probe_kind, "attic_cache_config", "real server uses canonical named-cache probe");
    assert(Object.hasOwn(constants, actual.stage), "real server returns an allowlisted stage");
    assert.equal(actual.write_auth_valid, null, "real read probe never establishes upload permission");
    await expect(result).toContainText(constants[actual.stage]);
    const patch = real.request().postDataJSON();
    assert(!Object.hasOwn(patch, "attic_token"), "real retained Test omits stored token");
    assert(patch.push_to == null, "real retained Test omits unchanged public URL");
    if (captureState) await captureState("attic-diagnostic-real");
    console.log(`TASK-470 real Attic diagnostic row ${id}: HTTP ${real.status()}, stage ${actual.stage}; no Save`);

    const cases = [
      ...["dns", "transport", "authentication", "cache_not_found", "response"].map(stage => ({ name: stage, status: 200, value: evidence(stage), text: constants[stage] })),
      { name: "private-read", status: 200, value: evidence("complete", true, true), text: constants.complete, token: "Token authentication for private cache read: Verified." },
      { name: "public-read", status: 200, value: evidence("complete", true), text: constants.complete },
      { name: "policy-400", status: 400, value: { ...evidence("target_policy"), error: "invalid_cache_test_config", details: null }, text: constants.target_policy },
      { name: "unknown-stage", status: 200, value: evidence("unsupported-stage", true, true), text: constants.unverified },
      { name: "unknown-kind", status: 200, value: { ...evidence("complete", true, true), probe_kind: "unsupported-kind" }, text: constants.unverified },
      { name: "legacy-root-success", status: 200, value: { success: true, message: forbidden, tested_url: forbiddenUrl }, text: constants.unverified },
      ...[401, 403].map(status => ({ name: `api-auth-${status}`, status, value: { ...evidence("target_policy"), error: "invalid_cache_test_config", details: null }, error: "Connection test failed. Check endpoint policy and credentials." })),
      { name: "non-policy-400", status: 400, value: { ...evidence("target_policy"), error: "forbidden", details: null }, error: "Connection test rejected. Check destination values and credentials." },
    ];
    for (const fixture of cases) {
      // Renderer-only interception. This is not native authentication evidence.
      const render = route => route.fulfill({ status: fixture.status, json: fixture.value });
      await page.route(probeUrl, render);
      try {
        const response = await click();
        assert.equal(response.status(), fixture.status, "renderer-only fixture HTTP status");
        if (fixture.error) {
          await expect(dialog.getByRole("alert")).toHaveText(fixture.error);
          await expect(result).toHaveCount(0);
        } else {
          await expect(result).toContainText(fixture.text);
          await expect(result).toContainText(fixture.token || "Token authentication: Untested.");
        }
        await expect(dialog).not.toContainText(forbidden);
        await expect(dialog).not.toContainText(forbiddenUrl);
        await expect(dialog).not.toContainText("fixture-forbidden-token");
        if (captureState) await captureState(`attic-diagnostic-renderer-${fixture.name}`);
      } finally { await page.unroute(probeUrl, render); }
    }
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    await parent.getByRole("button", { name: "Close", exact: true }).click();
    assert(JSON.stringify(await read()) === JSON.stringify(before), "diagnostic Tests leave GET configuration/timestamps unchanged");
    assert.equal(mutations.length, 0, "diagnostic renderer and real Test never create/update/delete cache configuration");
    console.log("TASK-470 Attic renderer-only stage/400/auth/no-echo cases passed");
  } finally { page.off("request", observe); }
}

module.exports = { sharedCacheModalWorkflow, atticProbeDiagnosticWorkflow };
