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

// I-BUG-04: a tela da escuta mostrava como válido um código que já tinha vencido no Rust.
test('código vencido aparece como vencido e a escuta relê o estado sozinha', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-settings-page')));
  await page.getByRole('button', { name: /Dispositivos/u }).click();
  await page.evaluate(() => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    window.__releituras = 0;
    const base = { escutando: true, porta: 45870, enderecos: ['192.168.1.145:45870'], ultimoResultado: null, ultimoErro: null };
    pagina.store.syncV2State.set({ ...base, pin: '1234 5678' });
    // O Rust venceu o código: a próxima leitura já vem sem ele.
    pagina.store.refreshSyncStatus = async () => { window.__releituras += 1; pagina.store.syncV2State.set({ ...base, pin: null }); };
  });
  await expect(page.locator('.pairing-panel')).toContainText('1234 5678');
  // I-BUG-08: no celular a escuta só vive com o app na tela.
  await expect(page.getByTestId('escuta-na-tela')).toContainText('NarraHub aberto na tela');
  await expect(page.getByTestId('pin-vencido')).toHaveCount(0);
  await expect(page.getByTestId('pin-vencido')).toBeVisible({ timeout: 8000 });
  await expect(page.getByTestId('pin-vencido')).toContainText('Novo código');
  expect(await page.evaluate(() => window.__releituras)).toBeGreaterThan(0);
});

// ── NH-084, PR B: pareamento por PIN assistido por QR ───────────────────────────────────────────
// A tela não interpreta o QR: desenha o conteúdo opaco que o Rust montou e devolve a leitura crua.
// A leitura é atalho — com ou sem câmera, o endereço e o código digitados continuam.

async function abrirDispositivos(page) {
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-settings-page')));
  await page.getByRole('button', { name: /Dispositivos/u }).click();
}

const ESCUTA = { escutando: true, porta: 45870, enderecos: ['192.168.1.145:45870'], ultimoResultado: null, ultimoErro: null };

test('o QR só abre quando pedido, só com código válido, e fecha quando o código vence', async ({ page }) => {
  await abrirDispositivos(page);
  await page.evaluate((escuta) => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    pagina.store.refreshSyncStatus = async () => undefined;
    pagina.store.syncV2State.set({ ...escuta, pin: '1234 5678' });
    pagina.store.syncV2Qr.set('conteudo-opaco-do-rust');
  }, ESCUTA);
  // Não abre sozinho: o escritor pede.
  await expect(page.getByTestId('mostrar-qr')).toBeVisible();
  await expect(page.getByTestId('pairing-qr')).toHaveCount(0);
  await page.getByTestId('mostrar-qr').click();
  await expect(page.getByTestId('dialogo-qr')).toBeVisible();
  await expect(page.getByTestId('dialogo-qr')).toContainText('192.168.1.145:45870');
  await expect(page.getByTestId('pairing-qr')).toBeVisible();
  const src = await page.getByTestId('pairing-qr').getAttribute('src');
  expect(src.startsWith('data:image/svg+xml')).toBe(true);

  await page.evaluate((escuta) => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    pagina.store.syncV2State.set({ ...escuta, pin: null });
  }, ESCUTA);
  await expect(page.getByTestId('dialogo-qr')).toHaveCount(0);
  await expect(page.getByTestId('pairing-qr')).toHaveCount(0);
  await expect(page.getByTestId('mostrar-qr')).toHaveCount(0);
  await expect(page.getByTestId('pin-vencido')).toBeVisible();

  // Código novo não reabre o QR sozinho.
  await page.evaluate((escuta) => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    pagina.store.syncV2State.set({ ...escuta, pin: '8765 4321' });
  }, ESCUTA);
  await expect(page.getByTestId('mostrar-qr')).toBeVisible();
  await expect(page.getByTestId('dialogo-qr')).toHaveCount(0);
});

// Teste físico da beta.10: durante a sessão os botões ficavam cinza sem dizer por quê.
test('durante a sessão a tela diz que está conectando', async ({ page }) => {
  await abrirDispositivos(page);
  await expect(page.getByTestId('sync-ocupado')).toHaveCount(0);
  await page.evaluate(() => window.ng.getComponent(document.querySelector('app-settings-page')).store.syncV2Busy.set(true));
  await expect(page.getByTestId('sync-ocupado')).toBeVisible();
  await expect(page.getByTestId('sync-ocupado')).toContainText('Conectando');
});

async function comLeitor(page, leitura) {
  await abrirDispositivos(page);
  await page.evaluate((leitura) => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    window.__pareamentosPorQr = [];
    window.__pareamentosManuais = 0;
    window.__permissoesAbertas = 0;
    pagina.qrScanner = {
      scan: async () => leitura,
      supported: async () => true,
      cancel: async () => undefined,
      openSettings: async () => { window.__permissoesAbertas += 1; },
    };
    pagina.qrScannerAvailable.set(true);
    pagina.store.pairSyncV2ByQr = async (conteudo) => {
      window.__pareamentosPorQr.push(conteudo);
      return { ok: true, endereco: '192.168.1.145:45870', result: { parceiro: { deviceId: 'D', nome: 'PC do Osmar' }, papel: 'par', houveBootstrap: false, blobsRecebidos: 0, eventosEnviados: 0, eventosAplicados: 0, eventosPendentes: 0 } };
    };
    pagina.store.pairSyncV2 = async () => { window.__pareamentosManuais += 1; return { ok: false, error: 'manual' }; };
    pagina.syncFeedback.workspaceSync.universeStore.load = async () => undefined;
    pagina.syncFeedback.workspaceSync.knowledgeStore.refreshLibraryPreviewTags = async () => undefined;
    pagina.syncFeedback.conflicts.refreshOpenCount = async () => undefined;
  }, leitura);
}

test('escanear QR pareia repassando a leitura crua, sem interpretar nada', async ({ page }) => {
  const cru = 'texto-cru-como-o-leitor-devolveu:1:9';
  await comLeitor(page, { kind: 'ok', conteudo: cru });
  await page.getByTestId('escanear-qr').click();
  await expect(page.locator('.toast')).toContainText('Sincronizado com PC do Osmar');
  expect(await page.evaluate(() => window.__pareamentosPorQr)).toEqual([cru]);
  // Teste físico da beta.10: depois do QR o endereço ficava vazio e "Sincronizar pareado", cinza.
  await expect(page.getByPlaceholder('192.168.0.10:45870')).toHaveValue('192.168.1.145:45870');
  await expect(page.getByRole('button', { name: 'Sincronizar pareado' })).toBeEnabled();
  expect(await page.evaluate(() => localStorage.getItem('narrahub.syncV2.lastAddress'))).toBe('192.168.1.145:45870');
});

test('o último endereço volta ao reabrir a tela', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('narrahub.syncV2.lastAddress', '192.168.1.145:45870'));
  await abrirDispositivos(page);
  await expect(page.getByPlaceholder('192.168.0.10:45870')).toHaveValue('192.168.1.145:45870');
});

test('câmera negada: nada é pareado e a entrada manual continua utilizável', async ({ page }) => {
  await comLeitor(page, { kind: 'negado' });
  await page.getByTestId('escanear-qr').click();
  await expect(page.getByTestId('qr-scan-message')).toContainText('Sem permissão para a câmera');
  expect(await page.evaluate(() => window.__pareamentosPorQr)).toEqual([]);
  // Teste físico da beta.11: depois de negar, o Android não pergunta de novo — a tela leva às permissões.
  await page.getByTestId('abrir-permissoes-camera').click();
  expect(await page.evaluate(() => window.__permissoesAbertas)).toBe(1);
  const endereco = page.getByPlaceholder('192.168.0.10:45870');
  await expect(endereco).toBeEditable();
  await endereco.fill('192.168.1.145:45870');
  await page.getByPlaceholder('0000 0000').fill('12345678');
  await page.getByRole('button', { name: 'Parear com código' }).click();
  await expect.poll(() => page.evaluate(() => window.__pareamentosManuais)).toBe(1);
});

for (const [nome, leitura, mensagem] of [
  ['cancelar o leitor', { kind: 'cancelado' }, null],
  ['leitor indisponível', { kind: 'indisponivel' }, 'Leitor de QR indisponível'],
]) {
  test(`${nome} nunca remove a entrada manual`, async ({ page }) => {
    await comLeitor(page, leitura);
    await page.getByPlaceholder('192.168.0.10:45870').fill('192.168.1.145:45870');
    await page.getByTestId('escanear-qr').click();
    if (mensagem) await expect(page.getByTestId('qr-scan-message')).toContainText(mensagem);
    expect(await page.evaluate(() => window.__pareamentosPorQr)).toEqual([]);
    await expect(page.getByPlaceholder('192.168.0.10:45870')).toHaveValue('192.168.1.145:45870');
    await expect(page.getByPlaceholder('0000 0000')).toBeEditable();
    await expect(page.getByRole('button', { name: 'Parear com código' })).toBeVisible();
  });
}

// Teste físico da beta.11: aberta a leitura, não havia como voltar ao app sem fechá-lo.
test('durante a leitura há mira e Cancelar, e cancelar devolve a tela inteira', async ({ page }) => {
  await comLeitor(page, { kind: 'cancelado' });
  await page.evaluate(() => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    window.__cancelamentos = 0;
    let terminar;
    pagina.qrScanner.scan = (aoAbrirCamera) => new Promise((resolve) => { terminar = resolve; aoAbrirCamera?.(); });
    pagina.qrScanner.cancel = async () => { window.__cancelamentos += 1; terminar({ kind: 'cancelado' }); };
  });
  await page.getByPlaceholder('192.168.0.10:45870').fill('192.168.1.145:45870');
  await page.getByTestId('escanear-qr').click();
  await expect(page.getByTestId('leitura-qr')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.classList.contains('nh-lendo-qr'))).toBe(true);
  // Nada opaco por cima da câmera: nem o fundo da página, nem no meio de uma transição.
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgba(0, 0, 0, 0)');
  // O resto da tela some para a câmera aparecer por baixo.
  await expect(page.getByPlaceholder('192.168.0.10:45870')).toBeHidden();
  await page.getByTestId('cancelar-leitura-qr').click();
  await expect(page.getByTestId('leitura-qr')).toHaveCount(0);
  expect(await page.evaluate(() => window.__cancelamentos)).toBe(1);
  expect(await page.evaluate(() => document.documentElement.classList.contains('nh-lendo-qr'))).toBe(false);
  await expect(page.getByPlaceholder('192.168.0.10:45870')).toHaveValue('192.168.1.145:45870');
  await expect(page.getByPlaceholder('192.168.0.10:45870')).toBeEditable();
});

test('sem leitor de QR, a tela é a de sempre', async ({ page }) => {
  await abrirDispositivos(page);
  await expect(page.getByTestId('escanear-qr')).toHaveCount(0);
  await expect(page.getByPlaceholder('192.168.0.10:45870')).toBeVisible();
});
