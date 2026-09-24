import { expect, test } from '@playwright/test';

/**
 * A tela de conflitos do Sync V2 (etapa F), em navegador real e sem Tauri.
 *
 * O frontend não decide nada: ele mostra os DTOs que o Rust entrega e devolve uma ação
 * portátil. Por isso o teste troca só a porta nativa (`SyncConflictsService`) por um dublê em
 * memória e mede o que a tela faz com os DTOs: lista, detalhe, as duas versões, EXCLUÍDO, diff,
 * a ação enviada, e que nada disso rola para o lado no celular.
 */

const agora = '2026-09-21T12:00:00Z';

const resumoEdicao = {
  conflictKey: 'k-edicao',
  kind: 'concurrent',
  descricao: 'Este capítulo foi editado nos dois aparelhos.',
  aggregateType: 'chapter',
  aggregateId: 'c1',
  rotuloDoTipo: 'Capítulo',
  titulo: 'Capítulo 1 — A chegada ao farol, com um título longo que precisa quebrar',
  universeId: 'u1',
  status: 'aberto',
  detectadoEm: agora,
  resolvidoEm: '',
  acaoInteira: false,
};

const resumoExclusao = {
  ...resumoEdicao,
  conflictKey: 'k-exclusao',
  descricao: 'Um aparelho editou e o outro excluiu.',
  aggregateId: 'c2',
  titulo: 'Capítulo 2',
};

const detalhes = {
  'k-edicao': {
    resumo: resumoEdicao,
    desteAparelho: {
      letra: 'a', desteAparelho: true, excluido: false, titulo: resumoEdicao.titulo, indisponivel: false,
      campos: [
        { campo: 'bookId', rotulo: 'bookId', valor: '06a7f83f-16d0-4d02-adcd-eef770866fba' },
        { campo: 'title', rotulo: 'Título', valor: resumoEdicao.titulo },
        { campo: 'status', rotulo: 'Estado', valor: 'IDEIA' },
        { campo: 'content', rotulo: 'Texto', valor: '<p>Era uma noite fria.</p><p>O vento batia.</p>' },
      ],
    },
    doOutro: {
      letra: 'b', desteAparelho: false, excluido: false, titulo: 'Capítulo 1', indisponivel: false,
      campos: [
        { campo: 'bookId', rotulo: 'bookId', valor: '06a7f83f-16d0-4d02-adcd-eef770866fba' },
        { campo: 'title', rotulo: 'Título', valor: 'Capítulo 1' },
        { campo: 'status', rotulo: 'Estado', valor: 'IDEIA' },
        { campo: 'content', rotulo: 'Texto', valor: '<p>Era uma noite fria.</p><p>A chuva caía sem parar sobre o farol abandonado.</p>' },
      ],
    },
    diferencas: [
      { campo: 'title', rotulo: 'Título', desteAparelho: resumoEdicao.titulo, doOutro: 'Capítulo 1' },
      { campo: 'content', rotulo: 'Texto', desteAparelho: '…', doOutro: '…' },
    ],
    diffDeTexto: [
      { tipo: 'igual', texto: 'Era uma noite fria.' },
      { tipo: 'so_deste', texto: 'O vento batia.' },
      { tipo: 'so_do_outro', texto: 'A chuva caía sem parar sobre o farol abandonado.' },
    ],
    acoes: [
      { acao: 'ficarComA', rotulo: 'Ficar com a versão deste aparelho', descricao: 'A outra versão sai.', pedeNome: false, tagId: '' },
      { acao: 'ficarComB', rotulo: 'Ficar com a versão do outro aparelho', descricao: 'Esta versão sai.', pedeNome: false, tagId: '' },
    ],
  },
  'k-exclusao': {
    resumo: resumoExclusao,
    desteAparelho: {
      letra: 'a', desteAparelho: true, excluido: false, titulo: 'Capítulo 2', indisponivel: false,
      campos: [{ campo: 'content', rotulo: 'Texto', valor: '<p>Texto que sobreviveu aqui.</p>' }],
    },
    doOutro: { letra: 'b', desteAparelho: false, excluido: true, titulo: '', indisponivel: false, campos: [] },
    diferencas: [],
    diffDeTexto: [],
    acoes: [
      { acao: 'restaurar', rotulo: 'Manter o conteúdo', descricao: 'O capítulo volta a existir.', pedeNome: false, tagId: '' },
      { acao: 'manterExclusao', rotulo: 'Aceitar a exclusão', descricao: 'O capítulo sai dos dois.', pedeNome: false, tagId: '' },
    ],
  },
};

async function abrirConflitos(page) {
  // A dica de primeira execução da navegação mobile cobre a tela; ela tem teste próprio.
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings/conflitos');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-conflicts-page')));
  await page.evaluate(({ resumos, detalhes }) => {
    const pagina = window.ng.getComponent(document.querySelector('app-conflicts-page'));
    const store = pagina.store;
    const estado = { abertos: [...resumos], resolvidas: [] };
    window.__resolucoes = estado.resolvidas;
    // O dublê da porta nativa: a mesma forma dos DTOs do Rust (o gate de contrato confere).
    store.service = {
      list: async (filtro = {}) => (filtro.status === 'resolvido' ? [] : estado.abertos),
      inspect: async (chave) => detalhes[chave],
      resolve: async (chave, acao) => {
        estado.resolvidas.push({ chave, acao });
        estado.abertos = estado.abertos.filter((c) => c.conflictKey !== chave);
        return { conflictKey: chave, resolutionRev: 'r-final' };
      },
      epochNotice: async () => null,
    };
    return store.load();
  }, { resumos: [resumoEdicao, resumoExclusao], detalhes });
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

test('lista, detalhe com as duas versões, diff e a decisão enviada como ação portátil', async ({ page }) => {
  await abrirConflitos(page);
  const pagina = page.getByTestId('conflicts-page');
  await expect(pagina.getByText('2 conflitos precisam da sua decisão.')).toBeVisible();
  await expect(page.getByTestId('conflito')).toHaveCount(2);
  await semRolagemHorizontal(page, 'lista');

  await page.getByTestId('conflito').first().click();
  const detalhe = page.getByTestId('conflito-detalhe');
  await expect(detalhe).toBeVisible();
  // No celular o detalhe fica abaixo da lista: abrir um conflito leva o escritor até ele.
  await expect(detalhe).toBeInViewport();
  await expect(detalhe.getByTestId('versao-deste')).toContainText('Neste aparelho');
  await expect(detalhe.getByTestId('versao-do-outro')).toContainText('No outro aparelho');
  // O texto chega sem marcação HTML.
  await expect(detalhe.getByTestId('versao-deste')).toContainText('Era uma noite fria.');
  await expect(detalhe.getByTestId('versao-deste')).not.toContainText('<p>');
  const diff = detalhe.getByTestId('diff-texto');
  await expect(diff.locator('.diff-so_deste')).toContainText('O vento batia.');
  await expect(diff.locator('.diff-so_do_outro')).toContainText('A chuva caía');
  // Campo a campo: o texto já está no diff de texto; o título aparece na tabela.
  await expect(detalhe.getByTestId('diff-campos')).toContainText('Título');
  await expect(detalhe.getByTestId('diff-campos').locator('tbody th', { hasText: 'Texto' })).toHaveCount(0);
  await semRolagemHorizontal(page, 'detalhe da edição');

  // As versões ficam uma sobre a outra no celular e lado a lado no desktop.
  const [deste, doOutro] = await Promise.all([
    detalhe.getByTestId('versao-deste').boundingBox(),
    detalhe.getByTestId('versao-do-outro').boundingBox(),
  ]);
  const largura = page.viewportSize().width;
  if (largura < 700) expect(doOutro.y, 'no celular, as versões empilham').toBeGreaterThan(deste.y + deste.height - 1);
  else expect(Math.abs(doOutro.y - deste.y), 'no desktop, as versões ficam lado a lado').toBeLessThan(2);

  // Os botões de decisão são alvos de toque de verdade.
  const botao = detalhe.getByTestId('acao-ficarComB');
  const caixa = await botao.boundingBox();
  expect(caixa.height).toBeGreaterThanOrEqual(36);

  await botao.click();
  await expect(page.locator('.conflicts-info')).toContainText('Decisão registrada');
  const enviadas = await page.evaluate(() => window.__resolucoes);
  expect(enviadas).toEqual([{ chave: 'k-edicao', acao: { tipo: 'ficarComB' } }]);
  await expect(page.getByTestId('conflito')).toHaveCount(1);
  await expect(pagina.getByText('1 conflito precisa da sua decisão.')).toBeVisible();
});

test('edição contra exclusão mostra EXCLUÍDO do lado que excluiu', async ({ page }) => {
  await abrirConflitos(page);
  await page.getByTestId('conflito').nth(1).click();
  const detalhe = page.getByTestId('conflito-detalhe');
  await expect(detalhe.getByTestId('versao-do-outro')).toContainText('EXCLUÍDO');
  await expect(detalhe.getByTestId('versao-deste')).not.toContainText('EXCLUÍDO');
  await expect(detalhe.getByTestId('versao-deste')).toContainText('Texto que sobreviveu aqui.');
  await expect(detalhe.getByTestId('acao-restaurar')).toBeVisible();
  await expect(detalhe.getByTestId('acao-manterExclusao')).toBeVisible();
  await semRolagemHorizontal(page, 'detalhe da exclusão');

  await detalhe.getByTestId('acao-manterExclusao').click();
  const enviadas = await page.evaluate(() => window.__resolucoes);
  expect(enviadas).toEqual([{ chave: 'k-exclusao', acao: { tipo: 'manterExclusao' } }]);
});

test('Configurações mostra quantos conflitos precisam de atenção e o aviso da atualização', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('narrahub.mobileNavigationHintSeen', '1'));
  await page.goto('/settings');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-settings-page')));
  // A sincronização vive numa aba; o cartão do Sync V2 fica nela.
  await page.getByRole('button', { name: /Dispositivos/u }).click();
  // Sem Tauri, a leitura inicial do aviso falha e zera o sinal; semeia depois dela.
  await page.waitForTimeout(300);
  await page.evaluate(() => {
    localStorage.removeItem('narrahub.syncEpochNoticeSeen');
    const pagina = window.ng.getComponent(document.querySelector('app-settings-page'));
    pagina.conflicts.openCount.set(3);
    pagina.conflicts.epochNotice.set({ pareamentosInvalidados: true, divergenciasArquivadas: 2, iniciadaEm: '2026-09-21T10:00:00Z' });
    pagina.epochNoticeDismissed.set(false);
    window.ng.applyChanges(pagina);
  });
  const link = page.getByTestId('link-conflitos');
  await expect(link).toContainText('3 conflitos precisam de atenção');
  const aviso = page.getByTestId('aviso-epoca');
  await expect(aviso).toContainText('A sincronização foi atualizada.');
  await expect(aviso).toContainText('Por segurança, pareie seus aparelhos novamente.');
  await expect(aviso).toContainText('2 conflitos da versão anterior foram guardados');
  await semRolagemHorizontal(page, 'configurações');

  await aviso.getByRole('button', { name: 'Entendi' }).click();
  await expect(aviso).toHaveCount(0);
  expect(await page.evaluate(() => localStorage.getItem('narrahub.syncEpochNoticeSeen'))).toBe('2026-09-21T10:00:00Z');

  await link.click();
  await page.waitForURL('**/settings/conflitos');
  await expect(page.getByTestId('conflicts-page')).toBeVisible();
});

// I-BUG-05 (Etapa I, achado em aparelho físico): a tela de conflitos não rolava — o contêiner da rota
// tem altura fixa e overflow oculto — e os valores saíam com 9px por colisão com uma classe global.
test('a tela de conflitos rola, lê-se sem lupa e esconde identificadores internos', async ({ page }) => {
  await abrirConflitos(page);
  await page.getByTestId('conflito').first().click();
  const detalhe = page.getByTestId('conflito-detalhe');
  await expect(detalhe).toBeVisible();

  // Rolar com a roda/gesto precisa mover a página: o botão de decisão tem de ser alcançável pelo usuário.
  const rolagem = await page.evaluate(() => {
    const host = document.querySelector('app-conflicts-page');
    // Conteúdo maior que a janela em qualquer viewport: a rolagem é da própria página.
    const extra = document.createElement('div');
    extra.style.height = '2000px';
    host.querySelector('.conflicts-page').appendChild(extra);
    return { overflowY: getComputedStyle(host).overflowY, cabe: host.scrollHeight <= host.clientHeight };
  });
  expect(rolagem.overflowY, 'a página de conflitos precisa rolar sozinha').toBe('auto');
  expect(rolagem.cabe).toBe(false);
  const caixa = await page.locator('app-conflicts-page').boundingBox();
  await page.mouse.move(caixa.x + caixa.width / 2, caixa.y + Math.min(caixa.height, 300) / 2);
  await page.mouse.wheel(0, 1200);
  await expect.poll(() => page.evaluate(() => document.querySelector('app-conflicts-page').scrollTop)).toBeGreaterThan(0);

  // Letra legível nas versões.
  const fonte = await detalhe.getByTestId('versao-deste').locator('.versao-texto p').first().evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
  expect(fonte).toBeGreaterThanOrEqual(13);

  // Identificador interno não é informação para o escritor; campo igual fica recolhido.
  await expect(detalhe.getByTestId('versao-deste')).not.toContainText('06a7f83f');
  await expect(detalhe.getByTestId('versao-deste')).not.toContainText('bookId');
  await expect(detalhe.getByTestId('versao-deste').locator('.versao-iguais')).toContainText('Estado');
  await expect(detalhe.getByTestId('versao-deste').locator('.versao-iguais')).not.toHaveAttribute('open', '');

  // O parágrafo que só existe de um lado vem destacado.
  await expect(detalhe.getByTestId('versao-deste').locator('.versao-texto p.difere')).toHaveText('O vento batia.');
  await expect(detalhe.getByTestId('versao-do-outro').locator('.versao-texto p.difere')).toHaveText('A chuva caía sem parar sobre o farol abandonado.');
});

test('capítulo em conflito: escolher uma versão e abrir no editor para ajustar', async ({ page }) => {
  await abrirConflitos(page);
  await page.getByTestId('conflito').first().click();
  const detalhe = page.getByTestId('conflito-detalhe');
  await detalhe.getByTestId('acao-ficarComB-editar').click();
  await expect(page).toHaveURL(/\/workspace\/u1\/writing\/c1$/u);
  const enviadas = await page.evaluate(() => window.__resolucoes);
  expect(enviadas).toEqual([{ chave: 'k-edicao', acao: { tipo: 'ficarComB' } }]);
});
