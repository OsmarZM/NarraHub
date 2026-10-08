import { expect, test } from '@playwright/test';

const backup = {
  formatVersion: 1, backupId: 'import-test', schemaVersion: 29, appVersion: '0.10.0-beta.15',
  createdAt: '2026-10-06T12:00:00Z', reason: 'manual',
  database: { file: 'narrahub.db', sha256: 'a'.repeat(64), sizeBytes: 4096 },
  assets: { count: 1, totalBytes: 1024, manifestSha256: 'b'.repeat(64), files: [] },
};

async function settings(page) {
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings');
  await page.waitForFunction(() => !!window.ng && !!document.querySelector('app-settings-page'));
  await page.evaluate((manifest) => {
    const component = window.ng.getComponent(document.querySelector('app-settings-page'));
    const store = component.store;
    window.__backupCalls = [];
    window.__testBackup = manifest;
    store.backupService.externalStatus = async () => ({ destination: null, enabled: false, lastSuccess: null, lastError: '' });
    store.backupService.list = async () => [manifest];
    store.backupService.prepareRestore = async () => {
      window.__backupCalls.push('prepare');
      return { token: 'token-test', backupId: manifest.backupId, safetyBackupId: 'safety', schemaVersion: 29, createdAt: manifest.createdAt, warnings: [] };
    };
    store.backupService.commitRestore = async () => { window.__backupCalls.push('commit'); };
    store.db.close = async () => { window.__backupCalls.push('close'); };
    store.backupService.restartAfterRestore = async () => { window.__backupCalls.push('restart'); };
    store.externalBackup.set({ destination: null, enabled: false, lastSuccess: null, lastError: '' });
  }, backup);
}

test('cancelar exportação e importação não anuncia sucesso nem restaura', async ({ page }) => {
  await settings(page);
  await page.evaluate(() => {
    const service = window.ng.getComponent(document.querySelector('app-settings-page')).store.backupService;
    service.exportExternal = async () => null;
    service.importExternal = async () => null;
  });
  await page.getByRole('button', { name: 'Salvar backup em outro local' }).click();
  await expect(page.getByText('Backup externo salvo e conferido por SHA-256.')).toHaveCount(0);
  await page.getByRole('button', { name: 'Restaurar de arquivo' }).click();
  await expect(page.getByRole('heading', { name: 'Restaurar este backup?' })).toHaveCount(0);
  expect(await page.evaluate(() => window.__backupCalls)).toEqual([]);
});

test('falha de destino ou salvamento não produz confirmação de cópia', async ({ page }) => {
  await settings(page);
  await page.evaluate(() => {
    const store = window.ng.getComponent(document.querySelector('app-settings-page')).store;
    store.backupService.exportExternal = async () => { throw new Error('Destino indisponível'); };
  });
  await page.getByRole('button', { name: 'Salvar backup em outro local' }).click();
  await expect(page.getByText('Destino indisponível', { exact: true })).toBeVisible();
  await page.evaluate(() => {
    const store = window.ng.getComponent(document.querySelector('app-settings-page')).store;
    store.manuscript.saveNow = async () => { store.manuscript.saveMessage.set('Erro ao salvar'); return null; };
    store.backupService.exportExternal = async () => { window.__backupCalls.push('export'); return null; };
  });
  await page.getByRole('button', { name: 'Salvar backup em outro local' }).click();
  await expect(page.getByText(/O capítulo não foi salvo\. Resolva/u)).toBeVisible();
  expect(await page.evaluate(() => window.__backupCalls)).toEqual([]);
});

test('importar mostra resumo e só substitui o acervo após confirmação digitada', async ({ page }) => {
  await settings(page);
  await page.evaluate(() => {
    window.ng.getComponent(document.querySelector('app-settings-page')).store.backupService.importExternal = async () => window.__testBackup;
  });
  await page.getByRole('button', { name: 'Restaurar de arquivo' }).click();
  await expect(page.getByRole('heading', { name: 'Restaurar este backup?' })).toBeVisible();
  expect(await page.evaluate(() => window.__backupCalls)).toEqual([]);
  await page.getByRole('button', { name: 'Preparar restauração' }).click();
  await expect(page.getByRole('button', { name: 'Restaurar e reiniciar' })).toBeDisabled();
  await page.locator('input[name="restoreConfirmation"]').fill('RESTAURAR');
  await page.getByRole('button', { name: 'Restaurar e reiniciar' }).click();
  expect(await page.evaluate(() => window.__backupCalls)).toEqual(['prepare', 'close', 'commit', 'restart']);
});

for (const theme of ['light', 'dark']) {
  test(`backup externo cabe no viewport e mantém alvos de toque no tema ${theme}`, async ({ page }, testInfo) => {
    await settings(page);
    await page.evaluate((mode) => {
      const component = window.ng.getComponent(document.querySelector('app-settings-page'));
      component.theme.setTheme(mode);
      component.store.externalBackup.set({ destination: 'D:/Backups/' + 'pasta-com-nome-muito-longo/'.repeat(10), enabled: true, lastSuccess: null, lastError: 'Permissão do destino revogada. Escolha a pasta novamente.' });
    }, theme);
    const card = page.locator('.backup-card');
    await card.scrollIntoViewIfNeeded();
    const geometry = await card.evaluate((element) => {
      const bounds = element.getBoundingClientRect();
      return { left: bounds.left, right: bounds.right, width: innerWidth, overflow: element.scrollWidth > element.clientWidth + 1,
        heights: [...element.querySelectorAll('.backup-actions button')].map((button) => button.getBoundingClientRect().height) };
    });
    expect(geometry.left).toBeGreaterThanOrEqual(0);
    expect(geometry.right).toBeLessThanOrEqual(geometry.width + 1);
    expect(geometry.overflow).toBe(false);
    expect(geometry.heights.every((height) => height >= 40)).toBe(true);
    await page.getByRole('button', { name: 'Salvar backup em outro local' }).scrollIntoViewIfNeeded();
    await page.screenshot({ path: `output/backup-${testInfo.project.name}-${theme}.png` });
  });
}

test('perfil vazio oferece recuperação antes da biblioteca e exige confirmação', async ({ page }) => {
  await settings(page);
  await page.evaluate(() => window.ng.getComponent(document.querySelector('app-root-layout')).bootstrap.needsInitialChoice.set(true));
  await expect(page.getByRole('heading', { name: 'Começar ou recuperar' })).toBeVisible();
  await expect(page.locator('app-settings-page')).toHaveCount(0);
  await page.evaluate(() => {
    const component = window.ng.getComponent(document.querySelector('app-initial-recovery'));
    component.service.importExternal = async () => window.__testBackup;
  });
  await page.getByRole('button', { name: 'Abrir backup externo' }).click();
  await expect(page.getByRole('heading', { name: 'Backup validado' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Restaurar e reiniciar' })).toBeDisabled();
  await page.getByRole('textbox').last().fill('RESTAURAR');
  await page.getByRole('button', { name: 'Restaurar e reiniciar' }).click();
  expect(await page.evaluate(() => window.__backupCalls)).toEqual(['prepare', 'commit', 'restart']);
});
