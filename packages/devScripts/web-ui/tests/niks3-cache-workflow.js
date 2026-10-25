/** TASK-470: isolated Niks3 form assertions shared by host and VM harnesses.
 * Successful Discovery/probes use native services; only failures are mocked. The
 * workflow deletes only its own uniquely named cache, never resets fixtures.
 */
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require("playwright");
const { expect } = require("@playwright/test");

async function niks3CacheWorkflow(page, baseUrl, apiBaseUrl, screenshot, captureState) {
  const name = `task470-ui-${Date.now()}`;
  assert(process.env.CF_CACHE_CREDENTIAL_FIXTURE, "Niks3 browser proof requires the private native fixture file");
  const fixture = JSON.parse(fs.readFileSync(process.env.CF_CACHE_CREDENTIAL_FIXTURE, "utf8"));
  const profile = fixture.niks3_form_checkpoint;
  assert(profile?.start_path && fixture.niks3.protected_profile_ready === true && fixture.production_basic.protected_profile_ready === true, "native profiles must be prepared without waiting for retained seed IDs");
  fs.writeFileSync(profile.start_path, JSON.stringify({ version: 1, workflow: "original-niks3-form" }), { mode: 0o600, flag: "wx" });
  await expect.poll(() => {
    try { const ack = JSON.parse(fs.readFileSync(profile.ack_path, "utf8")); return ack.ready === true && ack.workflow === "original-niks3-form"; }
    catch { return false; }
  }, { timeout: 60000, message: "independent native profile readiness before original Niks3 Tests" }).toBe(true);
  const native = fixture.niks3;
  const writeUrl = native.server_url;
  assert.notEqual(writeUrl, fixture.production_basic.server_url, "token probes use the native Bearer endpoint, never the mTLS-only relay");
  const readUrl = "https://cache:5753";
  const keys = native.public_keys;
  let discoveredReadUrl = native.substituter_url;
  const token = native.token;
  const rotatedToken = "fixture-niks3-rotated-token";
  const privateKey = native.write_client_key;
  const privateValues = [token, rotatedToken, privateKey, native.read_client_key];
  const requests = [];
  let createdId;
  let checkpointSequence = 0;
  const checkpoint = async phase => {
    const sequence = ++checkpointSequence;
    const target = fixture.niks3_form_checkpoint;
    assert(target, "native Niks3 raw-state checkpoint worker required");
    const temporary = `${target.request_path}.${sequence}`;
    fs.writeFileSync(temporary, JSON.stringify({ sequence, phase, id: createdId }), { mode: 0o600, flag: "wx" });
    fs.renameSync(temporary, target.request_path);
    await expect.poll(() => {
      try { const ack = JSON.parse(fs.readFileSync(target.ack_path, "utf8")); return ack.sequence === sequence && ack.phase === phase && ack.id === createdId && ack.ok === true; }
      catch { return false; }
    }, { timeout: 60000, message: "private Niks3 complete raw-state checkpoint" }).toBe(true);
  };
  let discoveryCount = 0;
  let failCreate = true;
  let failUpdate = false;
  let failDiscovery = true;
  let failTest = true;
  let failEnvironmentRead = false;
  let releaseDiscovery, releaseTest, releaseSave;
  const discoveryGate = new Promise(resolve => { releaseDiscovery = resolve; });
  const testGate = new Promise(resolve => { releaseTest = resolve; });
  const saveGate = new Promise(resolve => { releaseSave = resolve; });
  const mutationRoute = /\/api\/v1\/caches(?:\/\d+)?$/;
  // Change only the submitted scope, then forward to the real API. A failure
  // must roll back configuration and assignments together, not merely display
  // an intercepted error while leaving a global cache behind.
  await page.route(mutationRoute, async route => {
    const method = route.request().method();
    if ((method === "POST" && failCreate) || (method === "PUT" && failUpdate)) {
      if (method === "POST") { failCreate = false; await saveGate; }
      else failUpdate = false;
      const body = route.request().postDataJSON();
      assert(body.environment_ids.length > 0, "scope belongs in the mutation body");
      const response = await route.fetch({ postData: JSON.stringify({ ...body, environment_ids: ["ffffffff-ffff-4fff-8fff-ffffffffffff"] }) });
      assert(response.status() >= 400, "real API rejects nonexistent environment");
      console.log(`Real API rejected invalid-environment ${method}: HTTP ${response.status()}`);
      return route.fulfill({ response });
    }
    await route.continue();
  });
  const discoveryRoute = /\/api\/v1\/caches(?:\/\d+)?\/niks3\/discover$/;
  const testRoute = /\/api\/v1\/caches(?:\/\d+)?\/test-credentials$/;
  const environmentsRoute = /\/api\/v1\/caches\/\d+\/environments$/;
  const capture = request => { if (request.url().includes("/caches")) requests.push(request); };
  page.on("request", capture);
  await page.route(discoveryRoute, async route => {
    const body = route.request().postDataJSON();
    const stored = /\/caches\/\d+\//.test(route.request().url());
    if (!stored) assert.deepEqual(Object.keys(body), ["server_url"], "token discovery remains URL-only");
    assert.equal(stored ? body.niks3_server_url : body.server_url, writeUrl);
    for (const field of ["niks3_auth_token", "niks3_read_basic_username", "niks3_read_basic_password", "niks3_read_client_key"]) assert(!Object.hasOwn(body, field), `discovery omits ${field}`);
    discoveryCount++;
    if (failDiscovery) {
      failDiscovery = false;
      return route.fulfill({ status: 400, json: { error: "fixture-discovery-error" } });
    }
    await discoveryGate;
    await route.continue();
  });
  await page.route(testRoute, async route => {
    const body = route.request().postDataJSON();
    assert(["write", "read", "all"].includes(body.probe_scope), "Test declares its explicit scope");
    if (body.probe_scope === "write") {
      for (const field of ["niks3_read_client_key", "niks3_read_basic_username", "niks3_read_basic_password"]) assert(!Object.hasOwn(body, field), `write Test excludes ${field}`);
    } else {
      assert(!Object.hasOwn(body, "niks3_auth_token"), "read Test never transports a write bearer token");
      for (const field of ["niks3_write_client_cert", "niks3_write_client_key", "niks3_write_ca_cert"]) assert(!Object.hasOwn(body, field), `read Test excludes ${field}`);
    }
    if (failTest) { failTest = false; return route.fulfill({ status: 502, json: { error: token } }); }
    await testGate;
    await route.continue();
  });
  await page.route(environmentsRoute, async route => {
    if (route.request().method() === "GET" && failEnvironmentRead
      && await page.getByRole("dialog", { name: "Cache destination" }).count() > 0) {
      failEnvironmentRead = false;
      return route.fulfill({ status: 503, json: { error: "fixture assignment read failure" } });
    }
    assert.notEqual(route.request().method(), "PUT", "Niks3 Save must not issue a second assignment operation");
    await route.continue();
  });
  const csrf = async () => {
    // Match the production browser client: Chromium sends Secure loopback
    // cookies over HTTP, but Playwright's URL-filtered cookie list omits them.
    const value = await page.evaluate(() => document.cookie.split(";")
      .map(cookie => cookie.trim()).find(cookie => cookie.startsWith("__Host-cf-csrf="))
      ?.slice("__Host-cf-csrf=".length));
    assert(value, "authenticated browser CSRF cookie required");
    return { "X-CSRF-Token": value };
  };
  const api = async (method, url, data, expectedStatus) => {
    // Browser fetch preserves the fixture's secure loopback session-cookie
    // behavior. Node's APIRequestContext does not send that cookie over HTTP.
    const headers = await csrf();
    const response = await page.evaluate(async ({ url, method, headers, data }) => {
      const result = await fetch(url, { method, credentials: "include", headers: { ...headers, "Content-Type": "application/json" }, body: data === undefined ? undefined : JSON.stringify(data) });
      return { status: result.status, text: await result.text() };
    }, { url: `${apiBaseUrl}/api/v1${url}`, method, headers, data });
    if (expectedStatus !== undefined) assert.equal(response.status, expectedStatus, "real API expected status");
    else assert(response.status >= 200 && response.status < 300, `${method} ${url} status ${response.status}`);
    return { text: async () => response.text, json: async () => JSON.parse(response.text) };
  };
  const getRedacted = async () => {
    const response = await api("GET", `/caches/${createdId}`);
    const body = await response.text();
    for (const secret of privateValues) assert(!body.includes(secret), "GET must not leak private material");
    const cache = JSON.parse(body);
    for (const field of ["niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key"]) assert(!cache[field], `GET redacts ${field}`);
    return cache;
  };
  const openEdit = async () => {
    await page.goto(`${baseUrl}/caches`);
    await page.getByText(name, { exact: true }).click();
    const freshResponse = page.waitForResponse(r => r.url() === `${apiBaseUrl}/api/v1/caches/${createdId}` && r.request().method() === "GET");
    await page.getByRole("button", { name: "Edit cache", exact: true }).click();
    assert.equal((await freshResponse).status(), 200, "Niks3 Edit loads a fresh destination by ID");
    const dialog = page.getByRole("dialog", { name: "Cache destination" });
    await expect(dialog.getByLabel("Name", { exact: true })).toBeVisible();
    return dialog;
  };
  const save = async dialog => {
    await dialog.getByRole("button", { name: "Save changes", exact: true }).click();
    await expect(dialog).toBeHidden();
  };
  const identity = async (dialog, plane, action) => {
    await section(dialog, plane === "Write" ? "Write / API" : "Read / Pull");
    const trigger = dialog.getByRole("button", { name: /^(Add credential|Replace|Edit replacement)$/ });
    await trigger.click();
    const nested = page.getByRole("dialog", { name: `${plane} credential`, exact: true });
    await expect(nested).toBeVisible();
    await expect(nested).toHaveAttribute("aria-modal", "true");
    // The nested dialog must remain outside the inert outer shell.
    assert.equal(await nested.evaluate(node => !!node.closest("[inert]")), false);
    await action(nested);
    await nested.getByRole("button", { name: "Use for this cache", exact: true }).click();
    await expect(nested).toBeHidden();
    await expect(trigger).toBeFocused();
  };
  const section = (dialog, name) => dialog.getByRole("navigation").getByRole("button", { name, exact: true }).click();
  const mode = (dialog, plane, name) => dialog.getByRole("group", { name: `${plane} authentication`, exact: true }).getByRole("button", { name, exact: true }).click();
  const tokenDraft = (dialog, value) => identity(dialog, "Write", nested => nested.getByLabel("API token").fill(value));
  const evidence = (dialog, label) => dialog.getByText(label, { exact: true }).locator("..");
  const assertWrite = async (dialog, values) => {
    for (const [label, value] of [["API reachable", values[0]], ["Authentication", values[1]], ["Write authorization", values[2]]]) await expect(evidence(dialog, label)).toContainText(value);
    await expect(evidence(dialog, "Object publication")).toContainText("Untested");
    await expect(dialog).toContainText("publication remains Untested");
  };
  const nativeTest = async (dialog, plane) => {
    await section(dialog, plane === "write" ? "Write / API" : "Read / Pull");
    const response = page.waitForResponse(r => r.url() === `${apiBaseUrl}/api/v1/caches/${createdId}/test-credentials` && r.request().method() === "POST");
    await dialog.getByRole("button", { name: plane === "write" ? "Test write API" : "Test read endpoint", exact: true }).click();
    const reply = await response;
    assert.equal(reply.status(), 200, "CF structured probe transport status");
    const patch = reply.request().postDataJSON();
    assert.equal(patch.probe_scope, plane);
    for (const field of plane === "write" ? ["niks3_read_client_key", "niks3_read_basic_username", "niks3_read_basic_password"] : ["niks3_auth_token", "niks3_write_client_key", "niks3_write_client_cert"]) assert(!Object.hasOwn(patch, field), `scoped native request omits ${field}`);
    return await reply.json();
  };
  const parity = async dialog => {
    const viewport = page.viewportSize();
    const sections = ["Destination", "Write / API", "Read / Pull", "Trust", "Advanced"];
    try {
      for (const width of [1440, 860, 390]) {
        await page.setViewportSize({ width, height: width === 1440 ? 1000 : 844 });
        assert.equal(await page.evaluate(() => devicePixelRatio), 1, "design benchmark uses DPR1");
        assert.equal(await page.evaluate(() => window.visualViewport.scale), 1, "design benchmark uses 100% zoom");
        for (const label of sections) {
          const nav = dialog.getByRole("navigation");
          const button = nav.getByRole("button", { name: label, exact: true });
          await button.focus(); await page.keyboard.press("Enter");
          await expect(button).toHaveAttribute("aria-current", "true");
          const bounds = await dialog.boundingBox(), header = await dialog.locator("header").boundingBox(), footer = await dialog.locator("footer").boundingBox(), rail = await nav.boundingBox();
          assert(bounds && header && footer && rail);
          assert(bounds.x >= 0 && bounds.x + bounds.width <= width + 1 && bounds.y >= 0 && bounds.y + bounds.height <= (width === 1440 ? 1000 : 844) + 1, "dialog fits benchmark viewport");
          assert(Math.abs(header.y - bounds.y) <= 2 && footer.y + footer.height <= bounds.y + bounds.height + 1, "header/footer remain inside shell");
          assert.equal(await dialog.evaluate(node => node.scrollWidth <= node.clientWidth), true, "dialog has no horizontal overflow");
          if (width === 1440) { assert(Math.abs(bounds.width - 1120) <= 2); assert(Math.abs(rail.width - 236) <= 2); }
          else assert(rail.y >= header.y + header.height - 1 && Math.abs(rail.width - (bounds.width - 2)) <= 2, "860px breakpoint moves rail above body");
          for (const secret of ["Private key", "API token", "Password"]) await expect(dialog.getByLabel(secret, { exact: true })).toHaveCount(0);
          const tag = label.toLowerCase().replaceAll(/[^a-z]+/g, "-");
          if (captureState && width !== 860) await captureState(`niks3-native-parity-${tag}-${width}-dialog`);
          if (screenshot && width !== 860) await dialog.screenshot({ path: screenshot.replace(/\.png$/, `-native-${tag}-${width}-dialog.png`), animations: "disabled" });
        }
        await dialog.getByRole("button", { name: "Cancel", exact: true }).focus(); await page.keyboard.press("Tab");
        await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeFocused();
        await page.keyboard.press("Tab");
        assert(await dialog.evaluate(node => node.contains(document.activeElement)), "footer Tab wraps within dialog");
      }
    } finally { await page.setViewportSize(viewport); }
  };
  try {
    // Keep the overlay out of this form workflow without changing server-side
    // onboarding state. The harness owns this browser presentation record.
    await page.evaluate(() => localStorage.setItem("cf.coach.ui.v2", JSON.stringify({ panel: "dismissed", track: "setup" })));
    await page.goto(`${baseUrl}/caches`);
    const envBody = await (await api("GET", "/environments")).json();
    const envs = Array.isArray(envBody) ? envBody : envBody.environments;
    assert(envs.length >= 2, "canonical environment ordering proof requires two existing fixture environments");
    const selected = envs[0];
    await page.getByRole("button", { name: "Add cache", exact: true }).click();
    await page.getByLabel("Name", { exact: true }).fill(name);
    await page.getByRole("button", { name: "Nix HTTPS", exact: true }).click();
    await page.getByLabel("URL", { exact: true }).fill("https://retained-read.example.com");
    await page.getByRole("button", { name: "Environments", exact: true }).click();
    await page.getByRole("button", { name: selected.name, exact: true }).click();
    await page.getByRole("button", { name: "Destination", exact: true }).click();
    await page.getByRole("button", { name: "Niks3", exact: true }).click();
    let dialog = page.getByRole("dialog", { name: "Cache destination" });
    await expect(dialog).toBeVisible();
    // Wait for loaded scope before exercising the dedicated mount's baseline.
    // Retained common values seed this draft; a clean badge is not a saved row.
    await expect(dialog.getByLabel("Name", { exact: true })).toBeEnabled();
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toBeEnabled();
    await expect(dialog.locator("footer")).toContainText("1 selected");
    await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(name);
    await expect(dialog.locator("header")).toContainText(name);
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await dialog.getByLabel("Name", { exact: true }).fill(name + "x");
    await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(name + "x");
    await expect(dialog.locator("header")).toContainText("Unsaved changes");
    await dialog.getByLabel("Name", { exact: true }).fill(name);
    await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(name);
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    for (const name of ["Destination", "Write / API", "Read / Pull", "Trust", "Advanced"]) await expect(dialog.getByRole("navigation").getByRole("button", { name, exact: true })).toBeVisible();
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toHaveAttribute("aria-pressed", "true");
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue("");
    await section(dialog, "Destination");
    await dialog.getByLabel("Name", { exact: true }).fill("");
    await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
    await expect(dialog.getByTestId("cache-save-blocked")).toBeVisible();
    await expect(dialog.getByTestId("cache-save-blocked")).toContainText("Enter a cache name");
    assert(!requests.some(r => r.method() === "POST" && /\/caches$/.test(r.url())), "invalid Niks3 draft makes no create request");
    await dialog.getByLabel("Name", { exact: true }).fill(name);
    await dialog.getByRole("checkbox", { name: "Enabled", exact: true }).uncheck();
    await section(dialog, "Write / API");
    await expect(dialog.getByRole("group", { name: "Write authentication", exact: true }).getByRole("button", { name: "mTLS", exact: true })).toHaveAttribute("aria-pressed", "true");
    await mode(dialog, "Write", "API token");
    await tokenDraft(dialog, token);
    await section(dialog, "Read / Pull");
    await expect(dialog.getByRole("group", { name: "Read authentication", exact: true }).getByRole("button", { name: "Basic", exact: true })).toHaveAttribute("aria-pressed", "true");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://discarded-read.example.com");
    await section(dialog, "Destination");
    await dialog.getByRole("button", { name: "Attic", exact: true }).click();
    await expect(dialog.getByLabel("API token", { exact: true })).toHaveCount(0);
    await dialog.getByRole("button", { name: "Niks3", exact: true }).click();
    await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(name);
    await expect(dialog.getByRole("checkbox", { name: "Enabled", exact: true })).not.toBeChecked();
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toHaveAttribute("aria-pressed", "true");
    await dialog.getByRole("checkbox", { name: "Enabled", exact: true }).check();
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue("");
    await mode(dialog, "Read", "Public");
    await section(dialog, "Write / API");
    await mode(dialog, "Write", "API token");
    await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveValue("");
    const credentialOpener = dialog.getByRole("button", { name: "Add credential", exact: true });
    await credentialOpener.click();
    const emptyCredential = page.getByRole("dialog", { name: "Write credential", exact: true });
    await expect(emptyCredential.getByLabel("API token", { exact: true })).toHaveValue("");
    await expect(emptyCredential.getByRole("button", { name: "Use for this cache", exact: true })).toBeDisabled();
    await page.keyboard.press("Escape");
    await expect(emptyCredential).toBeHidden();
    await expect(credentialOpener).toBeFocused();
    await section(dialog, "Write / API");
    await dialog.getByLabel("Write / API URL", { exact: true }).fill(writeUrl);
    // Bootstrap discovery works before read URL, keys or token are entered.
    await dialog.getByRole("button", { name: "Discover", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Discovery failed");
    const blankDiscovery = page.waitForResponse(r => r.url().endsWith("/niks3/discover") && r.request().method() === "POST");
    const dirtyBeforeDiscovery = await dialog.locator("header").innerText();
    await dialog.getByRole("button", { name: "Discover", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Discovering…", exact: true })).toBeDisabled();
    await expect(dialog.getByLabel("Write / API URL")).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Read / Pull", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Cancel", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Close", exact: true })).toBeDisabled();
    releaseDiscovery();
    const blankReply = await blankDiscovery;
    assert.equal(blankReply.status(), 200);
    const blankMetadata = await blankReply.json();
    assert.equal(new URL(blankMetadata.substituter_url).href, new URL(native.substituter_url).href);
    assert.deepEqual(blankMetadata.public_keys, keys);
    discoveredReadUrl = blankMetadata.substituter_url;
    await expect(dialog.getByRole("status").filter({ hasText: "Discovery successful" })).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Apply discovered settings", exact: true })).toBeVisible();
    await expect(dialog.locator("header")).toHaveText(dirtyBeforeDiscovery, { useInnerText: true });
    await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveValue(writeUrl);
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue("");
    await section(dialog, "Trust");
    assert.deepEqual(await dialog.getByRole("textbox", { name: /^Signing public key / }).evaluateAll(nodes => nodes.map(node => node.value)), [""], "Discovery must leave blank keys unchanged");
    await section(dialog, "Write / API");
    if (captureState) await captureState("niks3-native-discovery-blank-proposal-unapplied");
    await dialog.getByRole("button", { name: "Apply discovered settings", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Apply discovered settings", exact: true })).toHaveCount(0);
    await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveValue(writeUrl);
    await expect(dialog.getByRole("group", { name: "Write authentication", exact: true }).getByRole("button", { name: "API token", exact: true })).toHaveAttribute("aria-pressed", "true");
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue(discoveredReadUrl);
    await expect(dialog.getByRole("group", { name: "Read authentication", exact: true }).getByRole("button", { name: "Public", exact: true })).toHaveAttribute("aria-pressed", "true");
    await section(dialog, "Trust");
    for (let i = 0; i < keys.length; i++) await expect(dialog.getByLabel(`Signing public key ${i + 1}`, { exact: true })).toHaveValue(keys[i]);
    assert.equal(discoveryCount, 2);
    // Discovery must not persist anything or transport any credentials.
    assert(!requests.some(r => r.method() === "POST" && /\/caches$/.test(r.url())));
    await tokenDraft(dialog, token);
    await dialog.getByRole("button", { name: "Test write API", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Test failed");
    await expect(dialog.getByRole("alert")).not.toContainText(token);
    const positiveWrite = page.waitForResponse(r => r.url().endsWith("/test-credentials") && r.request().method() === "POST");
    await dialog.getByRole("button", { name: "Test write API", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Testing…", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Edit replacement", exact: true })).toBeDisabled();
    releaseTest();
    const nativeWrite = await positiveWrite;
    assert.equal(nativeWrite.status(), 200);
    const nativeEvidence = await nativeWrite.json();
    assert.equal(nativeEvidence.ok, true);
    assert.equal(nativeEvidence.status_code, 400, "native missing closure is the protected positive response");
    for (const field of ["write_api_reachable", "write_authn_valid", "write_authorization_valid", "write_auth_valid"]) assert.equal(nativeEvidence[field], true);
    assert.equal(nativeEvidence.read_access_valid, null);
    const authorization = dialog.getByText("Write authorization", { exact: true }).locator("..");
    await expect(authorization).toContainText("Verified");
    await expect(dialog.getByText("Authentication", { exact: true }).locator("..")).toContainText("Verified");
    await expect(dialog.getByText("API reachable", { exact: true }).locator("..")).toContainText("Verified");
    await expect(dialog.getByText("Object publication", { exact: true }).locator("..")).toContainText("Untested");
    await section(dialog, "Read / Pull");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill(readUrl);
    await dialog.getByRole("button", { name: "Test read endpoint", exact: true }).click();
    await expect(dialog.getByText("Read access", { exact: true }).locator("..")).toContainText("Verified");
    // Discovery preserves conflicting operator-entered public values until an
    // explicit Apply. The token draft still must not enter the request.
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://operator-read.example.com");
    await section(dialog, "Write / API");
    await dialog.getByRole("button", { name: "Discover", exact: true }).click();
    await expect(dialog.getByRole("heading", { name: "Review discovered public metadata", exact: true })).toBeVisible();
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue("https://operator-read.example.com");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://operator-read.example.com/edited");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://operator-read.example.com");
    await section(dialog, "Write / API");
    await expect(dialog.getByRole("button", { name: "Apply discovered settings", exact: true })).toHaveCount(0);
    if (captureState) await captureState("niks3-native-discovery-read-edit-revert-clears-proposal");
    const renewedProposal = page.waitForResponse(r => r.url().endsWith("/niks3/discover") && r.request().method() === "POST");
    await dialog.getByRole("button", { name: "Discover", exact: true }).click();
    assert.equal((await renewedProposal).status(), 200);
    await expect(dialog.getByRole("button", { name: "Apply discovered settings", exact: true })).toBeVisible();
    const writeCredentialBeforeApply = await dialog.getByTestId("niks3-write-credential-state").innerText();
    await dialog.getByRole("button", { name: "Apply discovered settings", exact: true }).click();
    await expect(dialog.getByTestId("niks3-write-credential-state")).toHaveText(writeCredentialBeforeApply);
    await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveValue(writeUrl);
    await expect(dialog.locator("header")).toContainText("Unsaved changes");
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue(discoveredReadUrl);
    await section(dialog, "Trust");
    assert.deepEqual(await dialog.getByRole("textbox", { name: /^Signing public key / }).evaluateAll(nodes => nodes.map(node => node.value)), keys);
    await section(dialog, "Write / API");
    if (captureState) await captureState("niks3-native-discovery-explicit-apply-read-settings-only");
    if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-credentials.png"), fullPage: true, animations: "disabled" });
    if (captureState) await captureState("niks3-credentials");
    await section(dialog, "Advanced");
    for (const [label, value] of [["Parallel uploads", "1"], ["Retry attempts", "3"], ["Push timeout", "3600"]]) await expect(dialog.getByLabel(label, { exact: true })).toHaveValue(value);
    await expect(dialog.getByRole("checkbox", { name: "Require signatures", exact: true })).toBeChecked();
    await dialog.getByLabel("Push timeout", { exact: true }).fill("");
    await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
    await expect(dialog.getByTestId("cache-save-blocked")).toContainText("Enter whole-number Advanced values");
    await dialog.getByLabel("Push timeout", { exact: true }).fill("3600");
    await section(dialog, "Destination");
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toHaveAttribute("aria-pressed", "true");
    await dialog.getByRole("checkbox", { name: "Enabled", exact: true }).uncheck();
    await dialog.getByRole("button", { name: "Add cache", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Saving…", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toBeDisabled();
    releaseSave();
    await expect(dialog.getByRole("alert")).toContainText("Cache save failed");
    const list = await (await api("GET", "/caches")).json();
    const own = list.filter(c => c.name === name);
    assert.equal(own.length, 0, "failed scoped create leaves no global cache");
    if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-create-rollback.png"), fullPage: true, animations: "disabled" });
    if (captureState) await captureState("niks3-create-rollback");
    const createResponse = page.waitForResponse(r => /\/api\/v1\/caches$/.test(r.url()) && r.request().method() === "POST");
    await dialog.getByRole("button", { name: "Add cache", exact: true }).click();
    createdId = (await (await createResponse).json()).id;
    await expect(dialog).toBeHidden();
    const creates = requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url()));
    assert.equal(creates.length, 2, "one create request per Save attempt");
    assert.deepEqual(creates[1].postDataJSON().environment_ids, [selected.id]);
    assert.deepEqual(await (await api("GET", `/caches/${createdId}/environments`)).json(), [selected.id]);
    let cache = await getRedacted();
    assert.equal(cache.niks3_write_token_configured, true);
    assert.equal(cache.niks3_read_auth_mode, "none");
    assert.deepEqual(cache.niks3_public_keys, keys);
    dialog = await openEdit();
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await expect(dialog.getByRole("navigation").getByRole("button", { name: "Write / API", exact: true })).toContainText("API token");
    await expect(dialog.getByRole("navigation").getByRole("button", { name: "Read / Pull", exact: true })).toContainText("Public");
    await section(dialog, "Write / API");
    await expect(dialog.getByLabel("API token")).toHaveCount(0);
    await expect(dialog.getByTestId("niks3-write-credential-state")).toContainText("Current configured credential");
    await tokenDraft(dialog, rotatedToken);
    const beforeFailedUpdate = await getRedacted();
    await section(dialog, "Read / Pull");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://changed-read.example.com");
    const beforeUpdateCount = requests.filter(r => r.method() === "PUT").length;
    failUpdate = true;
    await dialog.getByRole("button", { name: "Save changes", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Cache save failed");
    assert.deepEqual(await getRedacted(), beforeFailedUpdate, "failed scoped update leaves configuration unchanged");
    assert.deepEqual(await (await api("GET", `/caches/${createdId}/environments`)).json(), [selected.id], "failed update retains existing scope");
    assert.equal(requests.filter(r => r.method() === "PUT").length, beforeUpdateCount + 1);
    if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-update-rollback.png"), fullPage: true, animations: "disabled" });
    if (captureState) await captureState("niks3-update-rollback");
    await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue("https://changed-read.example.com");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill(readUrl);
    await save(dialog);
    assert(requests.some(r => r.method() === "PUT" && r.postDataJSON()?.niks3_auth_token === rotatedToken), "rotation sends the replacement token");
    for (const request of requests.filter(r => r.method() === "PUT" && /\/caches\/\d+$/.test(r.url()))) {
      assert.deepEqual(request.postDataJSON().environment_ids, [selected.id], "every update carries selected scope");
    }
    cache = await getRedacted();
    assert(cache.niks3_write_token_configured);
    dialog = await openEdit();
    await section(dialog, "Write / API");
    await expect(dialog.getByLabel("API token")).toHaveCount(0);
    await save(dialog);
    cache = await getRedacted();
    assert(cache.niks3_write_token_configured, "blank edit retains token");
    dialog = await openEdit();
    await section(dialog, "Write / API");
    await mode(dialog, "Write", "mTLS");
    await section(dialog, "Read / Pull");
    await mode(dialog, "Read", "mTLS");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill(discoveredReadUrl);
    const updatesBeforeInvalidIdentity = requests.filter(r => r.method() === "PUT").length;
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
    await expect(dialog.getByTestId("cache-save-blocked")).toBeVisible();
    await expect(dialog.getByTestId("cache-save-blocked")).toContainText("Enter a client certificate and private key together");
    assert.equal(requests.filter(r => r.method() === "PUT").length, updatesBeforeInvalidIdentity, "invalid mTLS edit makes no update request");
    for (const plane of ["Write", "Read"]) {
      await identity(dialog, plane, async nested => {
        await nested.getByLabel("Client certificate", { exact: true }).fill(native[plane.toLowerCase() + "_client_cert"]);
        await nested.getByLabel("Private key", { exact: true }).fill(native[plane.toLowerCase() + "_client_key"]);
        await nested.getByLabel("Server CA bundle (optional)", { exact: true }).fill(native.ca_cert);
      });
    }
    await save(dialog);
    cache = await getRedacted();
    assert.equal(cache.niks3_write_token_configured, false);
    assert.equal(cache.niks3_write_mtls_configured, true);
    assert.equal(cache.niks3_read_mtls_configured, true);
    const otherEnvironment = envs.find(value => value.id !== selected.id);
    const twoScopes = [selected.id, otherEnvironment.id];
    await api("PUT", `/caches/${createdId}`, { environment_ids: twoScopes });
    assert.deepEqual((await (await api("GET", `/caches/${createdId}/environments`)).json()).sort(), [...twoScopes].sort());
    dialog = await openEdit();
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await checkpoint("before");
    await section(dialog, "Destination");
    const envButton = dialog.getByRole("button", { name: selected.name, exact: true });
    const otherButton = dialog.getByRole("button", { name: otherEnvironment.name, exact: true });
    await expect(otherButton).toHaveAttribute("aria-pressed", "true");
    const renderedColor = await envButton.evaluate(node => getComputedStyle(node).getPropertyValue("--n3-env-color").trim().toLowerCase());
    assert(/^#[0-9a-f]{6}$/i.test(selected.color_hex.trim()), "native environment has a valid actual color");
    assert.equal(renderedColor, selected.color_hex.trim().toLowerCase(), "pills render the actual environment color");
    assert(/^#[0-9a-f]{6}$/i.test(otherEnvironment.color_hex.trim()));
    assert.equal(await otherButton.evaluate(node => getComputedStyle(node).getPropertyValue("--n3-env-color").trim().toLowerCase()), otherEnvironment.color_hex.trim().toLowerCase());
    for (const id of twoScopes) assert(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id), "scope uses actual API UUIDs");
    await dialog.getByLabel("Name", { exact: true }).fill(name + "-draft");
    await expect(dialog.locator("header")).toContainText("Unsaved changes");
    await dialog.getByLabel("Name", { exact: true }).fill(name);
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await envButton.click();
    await otherButton.click();
    await expect(dialog.locator("footer")).toContainText("Global scope");
    await otherButton.click();
    await envButton.click();
    await expect(dialog.locator("footer")).toContainText("2 selected");
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await expect(dialog.getByRole("navigation").getByRole("button", { name: "Write / API", exact: true })).toContainText("mTLS");
    await expect(dialog.getByRole("navigation").getByRole("button", { name: "Read / Pull", exact: true })).toContainText("mTLS");
    await parity(dialog);
    await section(dialog, "Write / API");
    const provenWrite = await nativeTest(dialog, "write");
    assert.equal(provenWrite.ok, true); assert.equal(provenWrite.status_code, 400);
    for (const field of ["write_api_reachable", "write_authn_valid", "write_authorization_valid", "write_auth_valid"]) assert.equal(provenWrite[field], true);
    assert(["write_route_verified", "write_route_verified_certificate_gate"].includes(provenWrite.write_probe_diagnostic_code || provenWrite.diagnostic_code));
    await assertWrite(dialog, ["Verified", "Verified", "Verified"]);
    if (captureState) await captureState("niks3-native-protected-mtls-verified-publication-untested");
    await mode(dialog, "Write", "mTLS");
    await assertWrite(dialog, ["Verified", "Verified", "Verified"]);
    const replace = dialog.getByRole("button", { name: "Replace", exact: true });
    await replace.click();
    const retainedIdentity = page.getByRole("dialog", { name: "Write credential", exact: true });
    await expect(retainedIdentity.getByLabel("Private key", { exact: true })).toHaveValue("");
    await page.keyboard.press("Escape");
    await expect(replace).toBeFocused();
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await assertWrite(dialog, ["Verified", "Verified", "Verified"]);
    const provenRead = await nativeTest(dialog, "read");
    assert.equal(provenRead.ok, true); assert.equal(provenRead.read_access_valid, true); assert.equal(provenRead.signing_keys_valid, true);
    assert.equal(provenRead.write_api_reachable, null); assert.equal(provenRead.write_authorization_valid, null);
    await expect(evidence(dialog, "Read access")).toContainText("Verified");
    await section(dialog, "Write / API");
    await assertWrite(dialog, ["Verified", "Verified", "Verified"]);
    const storedDiscovery = page.waitForResponse(r => r.url() === `${apiBaseUrl}/api/v1/caches/${createdId}/niks3/discover` && r.request().method() === "POST");
    await dialog.getByRole("button", { name: "Discover", exact: true }).click();
    const discovered = await storedDiscovery;
    assert.equal(discovered.status(), 200);
    assert(!Object.hasOwn(discovered.request().postDataJSON(), "niks3_auth_token"));
    const publicMetadata = await discovered.json();
    assert.equal(publicMetadata.substituter_url, discoveredReadUrl);
    assert.deepEqual(publicMetadata.public_keys, keys);
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await expect(dialog.getByRole("button", { name: "Apply discovered settings", exact: true })).toHaveCount(0);
    if (captureState) await captureState("niks3-native-discovery-matching-edit-clean");
    await section(dialog, "Read / Pull");
    await expect(evidence(dialog, "Read access")).toContainText("Verified");
    await section(dialog, "Write / API");
    await identity(dialog, "Write", async nested => {
      await nested.getByLabel("Client certificate", { exact: true }).fill(native.read_client_cert);
      await nested.getByLabel("Private key", { exact: true }).fill(native.read_client_key);
      await nested.getByLabel("Server CA bundle (optional)", { exact: true }).fill(native.ca_cert);
    });
    const wrongSubject = await nativeTest(dialog, "write");
    assert.equal(wrongSubject.ok, false); assert.equal(wrongSubject.status_code, 401);
    assert.equal(wrongSubject.write_authn_valid, false); assert.equal(wrongSubject.write_authorization_valid, false);
    assert.equal(wrongSubject.write_probe_diagnostic_code || wrongSubject.diagnostic_code, "mtls_subject_rejected");
    await assertWrite(dialog, ["Verified", "Failed", "Failed"]);
    if (captureState) await captureState("niks3-native-mtls-subject-denied");
    await dialog.getByRole("button", { name: "Use current credential", exact: true }).click();
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    // An actual write-field edit invalidates green evidence even if reverted;
    // canonical dirty state still returns to clean without rebasing on probes.
    await dialog.getByLabel("Write / API URL", { exact: true }).fill(writeUrl + "/changed");
    await assertWrite(dialog, ["Untested", "Untested", "Untested"]);
    await dialog.getByLabel("Write / API URL", { exact: true }).fill(writeUrl);
    await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
    await mode(dialog, "Write", "API token");
    await tokenDraft(dialog, token);
    const tokenWrite = await nativeTest(dialog, "write");
    assert.equal(tokenWrite.ok, true); assert.equal(tokenWrite.status_code, 400);
    for (const field of ["write_api_reachable", "write_authn_valid", "write_authorization_valid", "write_auth_valid"]) assert.equal(tokenWrite[field], true);
    await assertWrite(dialog, ["Verified", "Verified", "Verified"]);
    if (captureState) await captureState("niks3-native-protected-token-verified-publication-untested");
    await tokenDraft(dialog, rotatedToken);
    const rejected = await nativeTest(dialog, "write");
    assert.equal(rejected.ok, false); assert.equal(rejected.status_code, 401);
    assert.equal(rejected.write_authn_valid, false); assert.equal(rejected.write_authorization_valid, false);
    assert.equal(rejected.write_probe_diagnostic_code || rejected.diagnostic_code, "token_rejected");
    await assertWrite(dialog, ["Verified", "Failed", "Failed"]);
    if (captureState) await captureState("niks3-native-token-denied-no-publication");
    await tokenDraft(dialog, token);
    await dialog.getByLabel("Write / API URL", { exact: true }).fill(writeUrl + "/unsupported/");
    const unsupported = await nativeTest(dialog, "write");
    assert.equal(unsupported.ok, false); assert.equal(unsupported.status_code, 404);
    assert.equal(unsupported.write_api_reachable, false, "generic 404 cannot validate public API metadata");
    assert.equal(unsupported.write_authn_valid, null); assert.equal(unsupported.write_authorization_valid, null);
    assert.equal(unsupported.write_probe_diagnostic_code || unsupported.diagnostic_code, "route_unsupported");
    await assertWrite(dialog, ["Failed", "Untested", "Untested"]);
    if (captureState) await captureState("niks3-native-unsupported-route-no-version-claim");
    await dialog.getByLabel("Write / API URL", { exact: true }).fill(writeUrl);
    const missing = await (await api("POST", `/caches/${createdId}/test-credentials`, {
      probe_scope: "write", niks3_write_auth_mode: "token", niks3_server_url: writeUrl,
    })).json();
    assert.equal(missing.ok, false); assert.equal(missing.write_api_reachable, null);
    assert.equal(missing.write_authn_valid, false); assert.equal(missing.write_authorization_valid, false);
    assert.equal(missing.write_probe_diagnostic_code || missing.diagnostic_code, "write_credentials_missing");
    await api("POST", "/caches/test-credentials", {
      cache_type: "Niks3", probe_scope: "all", niks3_server_url: writeUrl,
      push_to: readUrl, niks3_write_auth_mode: "token", niks3_read_auth_mode: "none",
      niks3_public_keys: keys,
    }, 400);
    // Renderer-only negative compatibility case: an older public-metadata
    // reply cannot turn absent protected-method evidence green. No successful
    // native authentication result is fulfilled or changed by this route.
    const legacyOnly = async route => route.fulfill({ json: {
      ok: false, message: "provider prose must not establish authorization", probe_scope: "write",
      server_reachable: true, status_code: 200, write_auth_valid: null,
    } });
    await page.route(testRoute, legacyOnly);
    try {
      await nativeTest(dialog, "write");
      await assertWrite(dialog, ["Untested", "Untested", "Untested"]);
      await expect(dialog).not.toContainText("provider prose must not establish authorization");
      if (captureState) await captureState("niks3-renderer-negative-legacy-public-metadata-only");
    } finally { await page.unroute(testRoute, legacyOnly); }
    await tokenDraft(dialog, rotatedToken);
    let releaseDenied, fetchedDenied, deliveredDenied;
    const heldDenial = new Promise(resolve => { releaseDenied = resolve; });
    const fetched = new Promise(resolve => { fetchedDenied = resolve; });
    const delivered = new Promise(resolve => { deliveredDenied = resolve; });
    let holdOnce = true;
    const delayedNativeDenial = async route => {
      if (!holdOnce || route.request().postDataJSON().probe_scope !== "write") return route.continue();
      holdOnce = false;
      const response = await route.fetch();
      assert.equal(response.status(), 200);
      const actual = await response.json();
      assert.equal(actual.ok, false); assert.equal(actual.status_code, 401);
      assert.equal(actual.write_authn_valid, false); assert.equal(actual.write_authorization_valid, false);
      fetchedDenied();
      await heldDenial;
      try { await route.fulfill({ response }); }
      catch { assert(route.request().failure(), "only a canceled old request may reject delivery"); }
      finally { deliveredDenied(); }
    };
    await page.route(testRoute, delayedNativeDenial);
    try {
      await dialog.getByRole("button", { name: "Test write API", exact: true }).click();
      await fetched;
      await assertWrite(dialog, ["Testing…", "Testing…", "Testing…"]);
      await expect(dialog.getByRole("button", { name: "Cancel", exact: true })).toBeDisabled();
      await expect(dialog.getByRole("button", { name: "Close", exact: true })).toBeDisabled();
      if (captureState) await captureState("niks3-native-denial-in-progress-locked");
      // Route navigation unmounts the old editor; the delayed native denial must
      // not overwrite a fresh editor's independent successful generation.
      dialog = await openEdit();
      const current = await nativeTest(dialog, "write");
      assert.equal(current.ok, true); assert.equal(current.status_code, 400);
      await assertWrite(dialog, ["Verified", "Verified", "Verified"]);
      releaseDenied(); await delivered;
      await assertWrite(dialog, ["Verified", "Verified", "Verified"]);
      await expect(dialog.locator("header")).not.toContainText("Unsaved changes");
      if (captureState) await captureState("niks3-native-stale-denial-cannot-overwrite-current");
    } finally { releaseDenied(); await page.unroute(testRoute, delayedNativeDenial); }
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(dialog).toBeHidden();
    await checkpoint("after");
    await api("PUT", `/caches/${createdId}`, { environment_ids: [selected.id] });
    assert.deepEqual(await (await api("GET", `/caches/${createdId}/environments`)).json(), [selected.id]);
    cache = await getRedacted();
    assert.equal(cache.niks3_write_mtls_configured, true, "native Test/Cancel does not rotate stored identity");
    dialog = await openEdit();
    await section(dialog, "Write / API");
    await expect(dialog.getByLabel("Private key")).toHaveCount(0);
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Private key")).toHaveCount(0);
    if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-mtls.png"), fullPage: true, animations: "disabled" });
    if (captureState) await captureState("niks3-mtls");
    await section(dialog, "Destination");
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toHaveAttribute("aria-pressed", "true");
    await save(dialog); // Blank identity replacements retain configured mTLS.
    cache = await getRedacted();
    assert(cache.niks3_write_mtls_configured && cache.niks3_read_mtls_configured);
    dialog = await openEdit();
    for (const plane of ["Write", "Read"]) await identity(dialog, plane, async nested => {
      await expect(nested.getByLabel("Client certificate", { exact: true })).toHaveValue("");
      await expect(nested.getByLabel("Private key", { exact: true })).toHaveValue("");
      await expect(nested.getByLabel("Server CA bundle (optional)", { exact: true })).toHaveValue("");
      await nested.getByRole("checkbox", { name: `Remove ${plane} server CA bundle on Save`, exact: true }).check();
    });
    await save(dialog);
    cache = await getRedacted();
    assert.equal(cache.niks3_write_ca_cert, null);
    assert.equal(cache.niks3_read_ca_cert, null);
    dialog = await openEdit();
    await section(dialog, "Write / API");
    await mode(dialog, "Write", "API token");
    await tokenDraft(dialog, token);
    await section(dialog, "Read / Pull");
    await mode(dialog, "Read", "Public");
    await save(dialog);
    cache = await getRedacted();
    assert(cache.niks3_write_token_configured);
    assert.equal(cache.niks3_write_mtls_configured, false);
    assert.equal(cache.niks3_read_mtls_configured, false);
    assert.equal(cache.niks3_write_client_cert, null);
    assert.equal(cache.niks3_read_client_cert, null);
    assert.deepEqual(await (await api("GET", `/caches/${createdId}/environments`)).json(), [selected.id]);
    // Basic is an independent read identity. Allowed whitespace survives the
    // request; GET and Edit expose neither username nor password.
    const basicPassword = fixture.production_basic.password;
    privateValues.push(basicPassword);
    dialog = await openEdit();
    await section(dialog, "Read / Pull");
    await mode(dialog, "Read", "Basic");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill(fixture.production_basic.substituter_url);
    await identity(dialog, "Read", async nested => {
      const confirm = nested.getByRole("button", { name: "Use for this cache", exact: true });
      await expect(confirm).toBeDisabled();
      await nested.getByLabel("Username", { exact: true }).fill("fixture:user");
      await nested.getByLabel("Password", { exact: true }).fill(basicPassword);
      await expect(confirm).toBeDisabled();
      await nested.getByLabel("Username", { exact: true }).fill(fixture.production_basic.username);
      await expect(confirm).toBeEnabled();
    });
    await save(dialog);
    const basicSave = requests.filter(r => r.method() === "PUT" && /\/caches\/\d+$/.test(r.url())).at(-1).postDataJSON();
    assert.equal(basicSave.niks3_read_basic_username, fixture.production_basic.username);
    assert.equal(basicSave.niks3_read_basic_password, basicPassword);
    cache = await getRedacted();
    assert.equal(cache.niks3_read_basic_configured, true);
    assert(!cache.niks3_read_basic_username && !cache.niks3_read_basic_password);
    const freshRoute = `${apiBaseUrl}/api/v1/caches/${createdId}`;
    const missingFlags = async route => {
      if (route.request().method() !== "GET") return route.continue();
      const response = await route.fetch();
      const value = await response.json();
      for (const field of ["niks3_write_token_configured", "niks3_read_basic_configured"]) delete value[field];
      await route.fulfill({ response, json: value });
    };
    await checkpoint("before");
    await page.route(freshRoute, missingFlags);
    try {
      dialog = await openEdit();
      await section(dialog, "Read / Pull");
      await expect(dialog.getByTestId("niks3-read-credential-state")).toContainText("Stored credential status unavailable");
      const retainedProbe = page.waitForRequest(r => r.url() === `${freshRoute}/test-credentials`);
      await dialog.getByRole("button", { name: "Test read endpoint", exact: true }).click();
      const patch = (await retainedProbe).postDataJSON();
      for (const field of ["niks3_read_basic_username", "niks3_read_basic_password", "niks3_auth_token"]) assert(!Object.hasOwn(patch, field), `retained read Test omits ${field}`);
      await expect(dialog.getByText("Read access", { exact: true }).locator("..")).toContainText("Verified");
      // A replacement is local; explicit Current restores server retention even
      // with missing flags. Cancel never sends a mutation.
      await identity(dialog, "Read", async nested => {
        await expect(nested.getByLabel("Username", { exact: true })).toHaveValue("");
        await expect(nested.getByLabel("Password", { exact: true })).toHaveValue("");
        await nested.getByLabel("Username", { exact: true }).fill("replacement user");
        await nested.getByLabel("Password", { exact: true }).fill("replacement password");
      });
      await dialog.getByRole("button", { name: "Use current credential", exact: true }).click();
      await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
      const mutationsBefore = requests.filter(r => r.method() === "PUT").length;
      await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
      assert.equal(requests.filter(r => r.method() === "PUT").length, mutationsBefore);
    } finally { await page.unroute(freshRoute, missingFlags); }
    await checkpoint("after");
    failEnvironmentRead = true;
    dialog = await openEdit();
    await expect(dialog.getByRole("alert")).toContainText("Environment assignments could not be loaded");
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
    await expect(dialog.locator("footer")).toContainText("Scope not loaded");
    await expect(dialog.locator("footer")).not.toContainText("Global scope");
    await section(dialog, "Write / API");
    await expect(dialog.getByRole("button", { name: "Test write API", exact: true })).toBeDisabled();
    if (captureState) await captureState("niks3-scope-load-error");
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    dialog = await openEdit();
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
    if (captureState) await captureState("niks3-destination");
    if (screenshot) {
      await page.screenshot({ path: screenshot, fullPage: true, animations: "disabled" });
      const viewport = page.viewportSize();
      await page.setViewportSize({ width: 390, height: 844 });
      await page.screenshot({ path: screenshot.replace(/\.png$/, "-mobile.png"), fullPage: true, animations: "disabled" });
      const bounds = await dialog.boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= 390, "mobile dialog fits viewport");
      await page.setViewportSize(viewport);
    }
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    for (const request of requests) {
      const url = request.url();
      for (const secret of privateValues) assert(!url.includes(secret), "secrets never occur in URLs");
    }
    const visibleText = await page.locator("body").innerText();
    assert(!requests.some(r => r.method() === "PUT" && /\/environments$/.test(r.url())), "no Save performs a second scope mutation");
    for (const secret of privateValues) assert(!visibleText.includes(secret), "secrets never occur in rendered text");
  } finally {
    releaseDiscovery(); releaseTest(); releaseSave();
    // Recover the owned ID if a later assertion fails immediately after create.
    if (!createdId) {
      try {
        const list = await (await api("GET", "/caches")).json();
        createdId = list.find(c => c.name === name)?.id;
      } catch { /* Preserve the original failure when login/session was lost. */ }
    }
    if (createdId) await api("DELETE", `/caches/${createdId}`);
    page.off("request", capture);
    await page.unroute(discoveryRoute);
    await page.unroute(testRoute);
    await page.unroute(environmentsRoute);
    await page.unroute(mutationRoute);
  }
}

module.exports = { niks3CacheWorkflow };

// Host feedback uses the repository web-ui-test launcher for Nix-provided
// Playwright/browser dependencies. It never starts or restarts services.
if (require.main === module) {
  (async () => {
    const [baseUrl, outputDir] = process.argv.slice(2);
    const apiBaseUrl = process.env.CF_UI_API_BASE_URL;
    assert(apiBaseUrl, "explicit fixture API origin required");
    const browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: "reduce" });
    await context.addInitScript(origin => localStorage.setItem("cf_backend_origin", origin), apiBaseUrl);
    // The dev banner can bypass the selected API origin. Permit only the
    // verified preview origins, regardless of the dynamically allocated ports.
    const previewOrigins = new Set([new URL(baseUrl).origin, new URL(apiBaseUrl).origin]);
    await context.route(/https?:\/\/(127\.0\.0\.1|localhost|\[::1\])(?::\d+)?\//, route =>
      previewOrigins.has(new URL(route.request().url()).origin) ? route.continue() : route.abort());
    const page = await context.newPage();
    let result;
    try {
      await page.goto(`${baseUrl}/login`);
      await page.getByPlaceholder("Enter your username").fill(process.env.CF_UI_TEST_USERNAME || "admin");
      await page.getByPlaceholder("Enter your password").fill(process.env.CF_UI_TEST_PASSWORD || "password");
      await page.getByRole("button", { name: "Sign In", exact: true }).click();
      await page.waitForURL(url => !url.pathname.includes("login"));
      const status = await page.request.get(`${apiBaseUrl}/status`);
      assert.equal(status.status(), 200);
      await require("./shared-cache-modal-workflow.js").sharedCacheModalWorkflow(page, baseUrl, apiBaseUrl, path.join(outputDir, "task470-shared.png"));
      await niks3CacheWorkflow(page, baseUrl, apiBaseUrl, path.join(outputDir, "task470-review-ui.png"));
      result = { name: "task470-niks3-cache", ok: true };
      console.log("TASK-470 Niks3 browser workflow passed against", apiBaseUrl);
    } catch (error) {
      result = { name: "task470-niks3-cache", ok: false, error: error.message };
      console.error(error);
      process.exitCode = 1;
    } finally {
      fs.writeFileSync(path.join(outputDir, "results.json"), JSON.stringify([result], null, 2));
      await context.close();
      await browser.close();
    }
  })().catch(error => { console.error(error); process.exitCode = 1; });
}
