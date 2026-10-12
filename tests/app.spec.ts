import { test, expect } from '@playwright/test';

test('app launches and renders main shell', async ({ page }) => {
  await page.goto('http://localhost:9666');
  await expect(page).toHaveTitle(/recode/i);
  await expect(page.locator('.app-root-loading')).not.toBeVisible({ timeout: 10000 });
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10000 });
  await expect(page.locator('.sidebar')).toBeVisible({ timeout: 5000 });
});

test('sidebar shows navigation items', async ({ page }) => {
  await page.goto('http://localhost:9666');
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10000 });
  await expect(page.locator('.nav-item', { hasText: 'Project' })).toBeVisible({ timeout: 5000 });
  await expect(page.locator('.nav-item', { hasText: 'Agent' })).toBeVisible();
  await expect(page.locator('.nav-item', { hasText: 'Runner' })).toBeVisible();
  await expect(page.locator('.nav-item', { hasText: 'Settings' })).toBeVisible();
});

test('navigation to Agent view works', async ({ page }) => {
  await page.goto('http://localhost:9666');
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10000 });
  await page.click('.nav-item >> text=Agent');
  await expect(page.locator('.topbar-title')).toContainText('Agent', { timeout: 5000 });
  await expect(page.locator('h1')).toContainText('Agent', { timeout: 5000 });
});

test('navigation to Settings view works', async ({ page }) => {
  await page.goto('http://localhost:9666');
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10000 });
  await page.click('.nav-item >> text=Settings');
  await expect(page.locator('.topbar-title')).toContainText('Settings', { timeout: 5000 });
});

test('navigation to Runner view works', async ({ page }) => {
  await page.goto('http://localhost:9666');
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10000 });
  await page.click('.nav-item >> text=Runner');
  await expect(page.locator('.topbar-title')).toContainText('Runner', { timeout: 5000 });
});

test('command palette opens with Ctrl+K', async ({ page }) => {
  await page.goto('http://localhost:9666');
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10000 });
  await page.keyboard.press('Control+k');
  await expect(page.locator('.palette')).toBeVisible({ timeout: 5000 });
  await expect(page.locator('.palette-input input')).toBeFocused();
});

test('theme picker is accessible from sidebar', async ({ page }) => {
  await page.goto('http://localhost:9666');
  await expect(page.locator('.app-shell')).toBeVisible({ timeout: 10000 });
  await page.click('text=Appearance');
  await expect(page.locator('.appearance-body')).toBeVisible({ timeout: 5000 });
  // Accent swatches should be present
  await expect(page.locator('.appearance-body button[aria-label="Violet"]')).toBeVisible();
  await expect(page.locator('.appearance-body button[aria-label="Emerald"]')).toBeVisible();
});
