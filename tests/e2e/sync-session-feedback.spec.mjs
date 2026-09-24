import { expect, test } from '@playwright/test';

/**
 * I-BUG-03 (Etapa I): o fim de uma sessão de sincronização precisa aparecer na tela e reler o acervo,
 * dos dois lados. Achado em aparelho físico: o Windows escutava, recebia o universo do celular e só
 * mostrava depois de reabrir; o celular idem com o universo do Windows; e nenhum dos dois dizia que
 * algo tinha acontecido.
 *
 * Sem Tauri: a porta nativa é trocada por um dublê que guarda o ouvinte do evento, e a releitura da
 * biblioteca é contada.
 */

const resultado = {
  parceiro: { deviceId: 'D1', nome: 'Celular de teste' },
  papel: 'par',
  houveBootstrap: false,
  blobsRecebidos: 0,
  eventosEnviados: 2,
  eventosAplicados: 1,
  eventosPendentes: 0,
};

async function prepararFeedback(page) {
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-settings-page')));
  await page.evaluate(async () => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    const feedback = pagina.syncFeedback;
    window.__recargas = 0;
    feedback.workspaceSync.universeStore.load = async () => { window.__recargas += 1; };
    feedback.workspaceSync.knowledgeStore.refreshLibraryPreviewTags = async () => undefined;
    feedback.settings.refreshSyncStatus = async () => undefined;
    feedback.conflicts.refreshOpenCount = async () => undefined;
    feedback.syncV2 = { onSessionServed: async (ouvinte) => { window.__sessaoAtendida = ouvinte; return () => undefined; } };
    feedback.stop();
    await feedback.start();
  });
}

test('sessão atendida pela escuta avisa na tela e relê a biblioteca', async ({ page }) => {
  await prepararFeedback(page);
  await page.evaluate((r) => window.__sessaoAtendida({ resultado: r, erro: null }), resultado);
  await expect(page.locator('.toast')).toContainText('Sincronizado com Celular de teste: 1 alteração recebida, 2 alterações enviadas');
  await expect.poll(() => page.evaluate(() => window.__recargas)).toBe(1);
});

test('sessão atendida que falha aparece como erro, sem fingir sucesso', async ({ page }) => {
  await prepararFeedback(page);
  await page.evaluate(() => window.__sessaoAtendida({ resultado: null, erro: 'conexão interrompida' }));
  await expect(page.getByRole('alert')).toContainText('Uma sincronização recebida não foi concluída: conexão interrompida');
  expect(await page.evaluate(() => window.__recargas)).toBe(0);
});

test('quem inicia o pareamento também vê o resultado e a biblioteca relida', async ({ page }) => {
  await prepararFeedback(page);
  await page.evaluate(async (r) => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    pagina.store.pairSyncV2 = async () => ({ ok: true, result: r });
    await pagina.pairSyncV2();
  }, resultado);
  await expect(page.locator('.toast')).toContainText('Sincronizado com Celular de teste');
  await expect.poll(() => page.evaluate(() => window.__recargas)).toBe(1);
});
