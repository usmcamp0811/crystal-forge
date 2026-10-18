/** Private assertions for the shared dialog_focus.rs boundary contract. */
const { expect } = require('@playwright/test');

async function assertDialogKeyboard(page, dialog, id) {
  await expect(dialog).toHaveAttribute('id', id);
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
  // Match the shared helper's enabled-control selector, excluding its sentinels.
  // Compare node identity on each poll. Resource updates can change a boundary
  // control's text or attributes without moving focus to a different control.
  const atEdge = boundary => dialog.evaluate((el, boundary) => {
    const nodes = [...el.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not(.cf-focus-sentinel)')]
      .filter(n => n.tabIndex >= 0 && !n.hasAttribute('disabled') && n.getAttribute('aria-hidden') !== 'true');
    return nodes.length > 0 && document.activeElement === (boundary === 'first' ? nodes[0] : nodes.at(-1));
  }, boundary);
  await expect.poll(() => atEdge('first'), { message: 'Initial focus must land on the first enabled dialog control' }).toBe(true);
  await page.keyboard.press('Shift+Tab');
  await expect.poll(() => atEdge('last'), { message: 'Shift+Tab must wrap to the last enabled dialog control' }).toBe(true);
  await page.keyboard.press('Tab');
  await expect.poll(() => atEdge('first'), { message: 'Tab must wrap to the first enabled dialog control' }).toBe(true);
  await page.keyboard.press('Tab');
  await expect.poll(() => dialog.evaluate(el => el.contains(document.activeElement) && !document.activeElement.classList.contains('cf-focus-sentinel'))).toBe(true);
}

async function dismissDialog(page, dialog, method, closeName = 'Close') {
  if (method === 'Escape') await page.keyboard.press('Escape');
  else if (method === 'X') await dialog.getByRole('button', { name: closeName, exact: true }).click();
  else {
    // The backdrop has no accessible role. This is its source-defined sibling.
    await page.locator('.fl-tray-backdrop').click({ position: { x: 2, y: 2 } });
  }
}

module.exports = { assertDialogKeyboard, dismissDialog };
