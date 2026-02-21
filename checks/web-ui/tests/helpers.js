async function assertVisible(page, selector) {
  await page.locator(selector).waitFor({ state: "visible", timeout: 5000 });
}

async function assertText(page, text) {
  await page
    .locator(`text=${text}`)
    .first()
    .waitFor({ state: "visible", timeout: 5000 });
}

module.exports = { assertVisible, assertText };
