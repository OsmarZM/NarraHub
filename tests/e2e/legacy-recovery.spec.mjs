import { expect, test } from '@playwright/test';

/**
 * A caixa de versões antigas (etapa H, H-R3), em navegador real e sem Tauri.
 *
 * O que os gates daqui provam é o que o backend não consegue provar sozinho: o aviso não pode ser
 * silenciado enquanto houver pendência, descartar exige confirmação explícita, e preservar nunca
 * sai sem um livro de destino escolhido.
 */

const itemPendente = {
  id: 'item-1',
  status: 'pending',
  aggregateType: 'chapter',
  aggregateId: 'cap-1',
  field: 'content',
  tituloAtual: 'A chegada ao farol',
  livroAtual: 'Livro I',
  versaoAtual: '<p>O texto que ficou neste aparelho.</p>',
  versaoAntiga: '<p>A versão que veio do outro aparelho e nunca foi decidida.</p>',
  registradoEm: '2025-02-02 10:00:00',
  resolvidoEm: '',
  capituloPreservado: '',
};

const itemPreservado = { ...itemPendente, id: 'item-2', status: 'preserved', tituloAtual: 'Capítulo 2', resolvidoEm: '2026-09-22 13:00:00', capituloPreservado: 'cap-novo' };

const destinos = [
  { bookId: 'b1', bookName: 'Livro I', storyName: 'A saga', universeId: 'u1', universeName: 'Hopi', eOLivroOriginal: true },
  { bookId: 'b2', bookName: 'Livro II', storyName: 'A saga', universeId: 'u1', universeName: 'Hopi', eOLivroOriginal: false },
];

async function abrirCaixa(page) {
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings/recuperacao-sync-antigo');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-legacy-recovery-page')));
  await page.evaluate(({ itens, destinos }) => {
    const pagina = window.ng.getComponent(document.querySelector('app-legacy-recovery-page'));
    const store = pagina.store;
    const estado = { itens: [...itens], acoes: [] };
    window.__acoes = estado.acoes;
    store.service = {
      pending: async () => estado.itens.filter((item) => item.status === 'pending').length,
      list: async () => estado.itens,
      destinations: async () => destinos,
      preserve: async (pedido) => {
        estado.acoes.push({ tipo: 'preservar', pedido });
        estado.itens = estado.itens.map((item) => (item.id === pedido.id ? { ...item, status: 'preserved' } : item));
        return 'cap-novo';
      },
      discard: async (item) => {
        estado.acoes.push({ tipo: 'descartar', item });
        estado.itens = estado.itens.map((atual) => (atual.id === item ? { ...atual, status: 'discarded' } : atual));
      },
    };
    return store.load();
  }, { itens: [itemPendente, itemPreservado], destinos });
}

async function semRolagemHorizontal(page, onde) {
  const medida = await page.evaluate(() => ({
    documento: document.documentElement.scrollWidth,
    corpo: document.body.scrollWidth,
    largura: window.innerWidth,
  }));
  expect(medida.documento, `${onde}: a página rola para o lado`).toBeLessThanOrEqual(medida.largura);
  expect(medida.corpo, `${onde}: o body rola para o lado`).toBeLessThanOrEqual(medida.largura);
}

test('lista as versões antigas, mostra as duas versões e preserva com livro e título', async ({ page }) => {
  await abrirCaixa(page);
  await expect(page.getByTestId('versao-antiga')).toHaveCount(2);
  await expect(page.getByTestId('recovery-page')).toContainText('1 versão antiga de capítulo ainda está guardada só neste aparelho');
  await semRolagemHorizontal(page, 'lista');

  await page.getByTestId('versao-antiga').first().click();
  const detalhe = page.getByTestId('versao-detalhe');
  await expect(detalhe.getByTestId('versao-atual')).toContainText('O texto que ficou neste aparelho.');
  await expect(detalhe.getByTestId('versao-guardada')).toContainText('A versão que veio do outro aparelho');
  await expect(detalhe.getByTestId('versao-guardada')).not.toContainText('<p>');
  await semRolagemHorizontal(page, 'detalhe');

  // O livro original vem pré-selecionado, e o título é editável antes de confirmar.
  await expect(detalhe.getByTestId('livro-destino')).toHaveValue('b1');
  await expect(detalhe.getByTestId('titulo-recuperado')).toHaveValue('A chegada ao farol — versão recuperada');
  await detalhe.getByTestId('titulo-recuperado').fill('Farol — versão do outro aparelho');
  await detalhe.getByTestId('acao-preservar').click();

  const acoes = await page.evaluate(() => window.__acoes);
  expect(acoes).toEqual([
    { tipo: 'preservar', pedido: { id: 'item-1', bookId: 'b1', titulo: 'Farol — versão do outro aparelho' } },
  ]);
  await expect(page.getByRole('status')).toContainText('preservada como capítulo novo');
});

test('descartar exige confirmação explícita', async ({ page }) => {
  await abrirCaixa(page);
  await page.getByTestId('versao-antiga').first().click();
  const detalhe = page.getByTestId('versao-detalhe');

  // O primeiro toque não descarta: ele mostra o que vai acontecer.
  await detalhe.getByTestId('acao-descartar').click();
  await expect(detalhe.getByTestId('confirmacao-descarte')).toContainText('deixará de aparecer como pendente');
  await expect(detalhe.getByTestId('confirmacao-descarte')).toContainText('registro histórico original continuará preservado');
  expect(await page.evaluate(() => window.__acoes)).toEqual([]);

  await detalhe.getByTestId('acao-descartar').click();
  expect(await page.evaluate(() => window.__acoes)).toEqual([{ tipo: 'descartar', item: 'item-1' }]);
});

test('o aviso em Configurações não some enquanto houver pendência', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-settings-page')));
  await page.getByRole('button', { name: /Dispositivos/u }).click();
  await page.waitForTimeout(300);
  await page.evaluate(() => {
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    pagina.legacyRecovery.pending.set(2);
    window.ng.applyChanges(pagina);
  });

  const aviso = page.getByTestId('aviso-versoes-antigas');
  await expect(aviso).toContainText('Este aparelho guarda 2 versões antigas de capítulos');
  await expect(aviso).toContainText('só existem aqui');
  await semRolagemHorizontal(page, 'configurações');

  // Não há "Entendi" nem nada que grave a dispensa: o aviso não tem como ser silenciado.
  await expect(aviso.getByRole('button')).toHaveCount(0);
  const guardado = await page.evaluate(() => JSON.stringify(Object.keys(localStorage)));
  expect(guardado).not.toContain('legacyRecovery');

  await aviso.getByTestId('link-versoes-antigas').click();
  await page.waitForURL('**/settings/recuperacao-sync-antigo');
  await expect(page.getByTestId('recovery-page')).toBeVisible();
});
