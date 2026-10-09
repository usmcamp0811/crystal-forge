/** TASK-470: isolated Niks3 form assertions shared by host and VM harnesses.
 * Discovery/probes are mocked; mutations use the selected fixture API. The
 * workflow deletes only its own uniquely named cache, never resets fixtures.
 */
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require("playwright");
const { expect } = require("@playwright/test");
const { generateKeyPairSync } = require("node:crypto");

async function niks3CacheWorkflow(page, baseUrl, apiBaseUrl, screenshot, captureState) {
  const name = `task470-ui-${Date.now()}`;
  const keys = ["cache-one", "cache-two"].map(label => `${label}:${Buffer.alloc(32, 7).toString("base64")}`);
  const token = "fixture-niks3-token-not-production";
  const rotatedToken = "fixture-niks3-rotated-token";
  const privateKey = generateKeyPairSync("rsa", { modulusLength: 2048, privateKeyEncoding: { type: "pkcs8", format: "pem" }, publicKeyEncoding: { type: "spki", format: "pem" } }).privateKey;
  // Frozen public X.509 fixture, shared with the server certificate tests.
  // The generated key is deliberately unrelated: these assertions exercise
  // persistence, not TLS handshakes. Real identities belong to the VM fixture.
  const cert = `-----BEGIN CERTIFICATE-----
MIIBYzCCARWgAwIBAgIUUiT5rFbIc6C8wVUsrDv3mmB3PDkwBQYDK2VwMCYxJDAi
BgNVBAMMG3Rhc2s0NzAtY2VydGlmaWNhdGUtZml4dHVyZTAgFw0yNjEwMDMwMDQ1
MTJaGA8yMTI2MDkwOTAwNDUxMlowJjEkMCIGA1UEAwwbdGFzazQ3MC1jZXJ0aWZp
Y2F0ZS1maXh0dXJlMCowBQYDK2VwAyEArmgxjwzB+VI7yxPnKKkVj9uulx0wtyrf
HC/p+1wXxpOjUzBRMB0GA1UdDgQWBBS5G+8DC/fImjFHjQtXKwSe/LTS7TAfBgNV
HSMEGDAWgBS5G+8DC/fImjFHjQtXKwSe/LTS7TAPBgNVHRMBAf8EBTADAQH/MAUG
AytlcANBANv0+9Ic9w11sJMah7/3hykwUUhs+iQqGZWTnWwQumZ7bkfKPl4Illm2
Zh/vQ6oHa2rNyo8ob+V7jS6Zzq1/6Qk=
-----END CERTIFICATE-----`;
  const privateValues = [token, rotatedToken, privateKey];
  const requests = [];
  let createdId;
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
    assert.equal(stored ? body.niks3_server_url : body.server_url, "https://write.example.com");
    for (const field of ["niks3_auth_token", "niks3_read_basic_username", "niks3_read_basic_password", "niks3_read_client_key"]) assert(!Object.hasOwn(body, field), `discovery omits ${field}`);
    discoveryCount++;
    if (failDiscovery) {
      failDiscovery = false;
      return route.fulfill({ status: 400, json: { error: "fixture-discovery-error" } });
    }
    await discoveryGate;
    await route.fulfill({ json: { server_url: "https://write.example.com", substituter_url: "https://read.example.com", public_keys: keys, oidc_audience: "fixture-oidc-not-offered" } });
  });
  await page.route(testRoute, async route => {
    const body = route.request().postDataJSON();
    assert(["write", "read"].includes(body.probe_scope), "Test explicitly selects one plane");
    assert(!Object.hasOwn(body, "niks3_auth_token"), "metadata probes never transport a bearer token");
    if (body.probe_scope === "write") {
      for (const field of ["niks3_read_client_key", "niks3_read_basic_username", "niks3_read_basic_password"]) assert(!Object.hasOwn(body, field), `write Test excludes ${field}`);
    } else {
      for (const field of ["niks3_write_client_cert", "niks3_write_client_key", "niks3_write_ca_cert"]) assert(!Object.hasOwn(body, field), `read Test excludes ${field}`);
    }
    if (failTest) { failTest = false; return route.fulfill({ status: 502, json: { error: token } }); }
    await testGate;
    await route.fulfill({ json: { success: true, message: "Public metadata/read checks passed", status_code: 200, tested_url: "https://read.example.com", write_api_reachable: body.probe_scope === "write" ? true : null, write_authn_valid: null, write_authorization_valid: null, read_access_valid: body.probe_scope === "read" ? true : null, signing_keys_valid: body.probe_scope === "read" ? true : null } });
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
    const cookies = await page.context().cookies(apiBaseUrl);
    const cookie = cookies.find(c => c.name.includes("csrf"));
    return cookie ? { "X-CSRF-Token": cookie.value } : {};
  };
  const api = async (method, url, data) => {
    // Browser fetch preserves the fixture's secure loopback session-cookie
    // behavior. Node's APIRequestContext does not send that cookie over HTTP.
    const headers = await csrf();
    const response = await page.evaluate(async ({ url, method, headers, data }) => {
      const result = await fetch(url, { method, credentials: "include", headers: { ...headers, "Content-Type": "application/json" }, body: data === undefined ? undefined : JSON.stringify(data) });
      return { status: result.status, text: await result.text() };
    }, { url: `${apiBaseUrl}/api/v1${url}`, method, headers, data });
    assert(response.status >= 200 && response.status < 300, `${method} ${url} status ${response.status}`);
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
  try {
    // Keep the overlay out of this form workflow without changing server-side
    // onboarding state. The harness owns this browser presentation record.
    await page.evaluate(() => localStorage.setItem("cf.coach.ui.v2", JSON.stringify({ panel: "dismissed", track: "setup" })));
    await page.goto(`${baseUrl}/caches`);
    const envBody = await (await api("GET", "/environments")).json();
    const envs = Array.isArray(envBody) ? envBody : envBody.environments;
    assert(envs.length > 0, "fixture requires an environment");
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
    await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(name);
    await expect(dialog.locator("header")).toContainText(name);
    await expect(dialog.locator("header")).toContainText("Unsaved changes");
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
    await dialog.getByLabel("Write / API URL", { exact: true }).fill("https://write.example.com");
    // Bootstrap discovery works before read URL, keys or token are entered.
    await dialog.getByRole("button", { name: "Discover", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Discovery failed");
    await dialog.getByRole("button", { name: "Discover", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Discovering…", exact: true })).toBeDisabled();
    await expect(dialog.getByLabel("Write / API URL")).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Read / Pull", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Cancel", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Close", exact: true })).toBeDisabled();
    releaseDiscovery();
    await expect(dialog.getByRole("status").filter({ hasText: "Discovery successful" })).toBeVisible();
    await section(dialog, "Read / Pull");
    await expect(dialog.getByLabel("Read / substituter URL")).toHaveValue("https://read.example.com");
    await section(dialog, "Trust");
    for (let i = 0; i < keys.length; i++) await expect(dialog.getByLabel(`Signing public key ${i + 1}`, { exact: true })).toHaveValue(keys[i]);
    assert.equal(discoveryCount, 2);
    // Discovery must not persist anything or transport any credentials.
    assert(!requests.some(r => r.method() === "POST" && /\/caches$/.test(r.url())));
    await tokenDraft(dialog, token);
    await dialog.getByRole("button", { name: "Test write API", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Test failed");
    await expect(dialog.getByRole("alert")).not.toContainText(token);
    await dialog.getByRole("button", { name: "Test write API", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Testing…", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: "Edit replacement", exact: true })).toBeDisabled();
    releaseTest();
    const authorization = dialog.getByText("Write authorization", { exact: true }).locator("..");
    await expect(authorization).toContainText("Untested");
    await expect(authorization).not.toContainText("Verified");
    await section(dialog, "Read / Pull");
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
    await section(dialog, "Write / API");
    await dialog.getByRole("button", { name: "Apply discovered metadata", exact: true }).click();
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
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://read.example.com");
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
    const updatesBeforeInvalidIdentity = requests.filter(r => r.method() === "PUT").length;
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
    await expect(dialog.getByTestId("cache-save-blocked")).toBeVisible();
    await expect(dialog.getByTestId("cache-save-blocked")).toContainText("Enter a client certificate and private key together");
    assert.equal(requests.filter(r => r.method() === "PUT").length, updatesBeforeInvalidIdentity, "invalid mTLS edit makes no update request");
    for (const plane of ["Write", "Read"]) {
      await identity(dialog, plane, async nested => {
        await nested.getByLabel("Client certificate", { exact: true }).fill(cert);
        await nested.getByLabel("Private key", { exact: true }).fill(privateKey);
        await nested.getByLabel("Server CA bundle (optional)", { exact: true }).fill(cert);
      });
    }
    await save(dialog);
    cache = await getRedacted();
    assert.equal(cache.niks3_write_token_configured, false);
    assert.equal(cache.niks3_write_mtls_configured, true);
    assert.equal(cache.niks3_read_mtls_configured, true);
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
    const basicPassword = "  fixture-read-password  ";
    privateValues.push(basicPassword);
    dialog = await openEdit();
    await section(dialog, "Read / Pull");
    await mode(dialog, "Read", "Basic");
    await identity(dialog, "Read", async nested => {
      const confirm = nested.getByRole("button", { name: "Use for this cache", exact: true });
      await expect(confirm).toBeDisabled();
      await nested.getByLabel("Username", { exact: true }).fill("fixture:user");
      await nested.getByLabel("Password", { exact: true }).fill(basicPassword);
      await expect(confirm).toBeDisabled();
      await nested.getByLabel("Username", { exact: true }).fill("fixture user");
      await expect(confirm).toBeEnabled();
    });
    await save(dialog);
    const basicSave = requests.filter(r => r.method() === "PUT" && /\/caches\/\d+$/.test(r.url())).at(-1).postDataJSON();
    assert.equal(basicSave.niks3_read_basic_username, "fixture user");
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
