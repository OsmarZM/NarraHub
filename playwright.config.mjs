import { defineConfig, devices } from '@playwright/test';

/**
 * Testes da experiência mobile em navegador real (Chromium), não em análise estática.
 *
 * Quatro celulares Android com toque e o desktop. O app roda no `ng serve` em modo de
 * desenvolvimento, sem Tauri: os testes semeiam dados falsos pelas devtools do Angular
 * (`window.ng`). O que o Chromium não emula — teclado real do Android, pinça, gesto voltar do
 * sistema, insets do WebView — está no roteiro físico (docs/mobile/ROTEIRO_ANDROID.md).
 */
const android = (width, height) => ({
  ...devices['Pixel 7'],
  viewport: { width, height },
  screen: { width, height },
  isMobile: true,
  hasTouch: true,
});

export default defineConfig({
  testDir: './tests/e2e',
  testMatch: /.*\.spec\.mjs/u,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]] : 'list',
  use: {
    baseURL: 'http://127.0.0.1:4310',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'android-375x667', use: android(375, 667) },
    { name: 'android-390x844', use: android(390, 844) },
    { name: 'android-412x915', use: android(412, 915) },
    { name: 'android-430x932', use: android(430, 932) },
    { name: 'desktop-1366x768', use: { ...devices['Desktop Chrome'], viewport: { width: 1366, height: 768 } } },
  ],
  webServer: {
    command: 'npx ng serve --host 127.0.0.1 --port 4310',
    url: 'http://127.0.0.1:4310',
    reuseExistingServer: !process.env.CI,
    timeout: 240_000,
  },
});
