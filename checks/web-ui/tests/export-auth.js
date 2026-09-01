const DEFAULT_USER = Object.freeze({
  username: "admin",
  email: "admin@example.com",
  password: "testpassword123",
  firstName: "Test",
  lastName: "Admin",
});

async function isAuthenticated(page, apiBaseUrl) {
  return page.evaluate(async (base) => {
    const response = await fetch(`${base}/api/auth/whoami`, {
      credentials: "include",
    });
    if (!response.ok) return false;
    const auth = await response.json();
    return auth.is_authenticated === true;
  }, apiBaseUrl);
}

/**
 * Ensures that the export-test browser context has an authenticated local admin.
 *
 * The export checks validate exported documents, not the registration form. They
 * use the local-auth API so registration-form markup changes cannot prevent the
 * isolated checks from reaching their export assertions.
 */
async function ensureLocalAdmin(page, baseUrl, user = DEFAULT_USER) {
  const apiBaseUrl = new URL(baseUrl).origin;
  await page.context().addInitScript(() => {
    window.localStorage.setItem("cf.coach.collapsed", "true");
    window.localStorage.setItem("cf.coach.force_show", "false");
  });
  await page.goto(`${baseUrl}/login`, {
    timeout: 10000,
    waitUntil: "domcontentloaded",
  });

  if (await isAuthenticated(page, apiBaseUrl)) return;

  const setupStatus = await page.evaluate(async (base) => {
    const response = await fetch(`${base}/api/auth/setup-status`, {
      credentials: "include",
    });
    return {
      ok: response.ok,
      status: response.status,
      body: await response.text(),
    };
  }, apiBaseUrl);
  if (!setupStatus.ok) {
    throw new Error(
      `Authentication setup-status request failed (${setupStatus.status}): ${setupStatus.body}`,
    );
  }

  const setup = JSON.parse(setupStatus.body);
  if (setup.requires_setup === true) {
    const registration = await page.evaluate(async ({ base, account }) => {
      const response = await fetch(`${base}/api/auth/local/register`, {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          username: account.username,
          email: account.email,
          password: account.password,
          first_name: account.firstName,
          last_name: account.lastName,
        }),
      });
      return {
        ok: response.ok,
        status: response.status,
        body: await response.text(),
      };
    }, { base: apiBaseUrl, account: user });
    if (!registration.ok && registration.status !== 409) {
      throw new Error(
        `Administrator registration failed (${registration.status}): ${registration.body}`,
      );
    }
    if (await isAuthenticated(page, apiBaseUrl)) return;
  }

  const login = await page.evaluate(async ({ base, account }) => {
    const response = await fetch(`${base}/api/auth/local/login`, {
      method: "POST",
      credentials: "include",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        username: account.username,
        password: account.password,
      }),
    });
    return {
      ok: response.ok,
      status: response.status,
      body: await response.text(),
    };
  }, { base: apiBaseUrl, account: user });
  if (!login.ok) {
    throw new Error(`Administrator login failed (${login.status}): ${login.body}`);
  }
  if (!(await isAuthenticated(page, apiBaseUrl))) {
    throw new Error("Administrator login completed without an authenticated session");
  }
}

module.exports = { ensureLocalAdmin };
