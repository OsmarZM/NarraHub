import { expect, test } from '@playwright/test';

const ehDesktop = (testInfo) => testInfo.project.name.startsWith('desktop');

// ── apoio ─────────────────────────────────────────────────────────────────────

async function abrirBiblioteca(page) {
  await page.addInitScript(() => {
    // A dica de primeira execução cobriria a alça nos testes de gesto; ela tem teste próprio.
    if (!sessionStorage.getItem('nh-e2e-dica')) localStorage.setItem('narrahub.mobileNavigationHintSeen', '1');
  });
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.ng && document.querySelector('app-library-page')));
}

/** Semeia um universo com história, livro e três capítulos, e abre a escrita. Sem Tauri. */
async function abrirEscrita(page) {
  await abrirBiblioteca(page);
  await page.evaluate(() => {
    const agora = new Date().toISOString();
    window.ng.getComponent(document.querySelector('app-library-page')).store.universes.set([
      { id: 'u1', name: 'Hopi horror', description: '', cover_image: null, created_at: agora, updated_at: agora, last_opened_at: agora },
    ]);
  });
  await page.getByRole('button', { name: /Abrir universo/u }).click();
  await page.waitForFunction(() => Boolean(document.querySelector('app-writing-page')));
  await page.evaluate(() => {
    const agora = new Date().toISOString();
    const pagina = window.ng.getComponent(document.querySelector('app-writing-page'));
    const store = pagina.store;
    const historia = { id: 's1', universe_id: 'u1', name: 'A saga do farol', description: '', position: 0, created_at: agora, updated_at: agora };
    const livro = { id: 'b1', universe_id: 'u1', story_id: 's1', name: 'Livro I', description: '', cover_image: null, position: 0, created_at: agora, updated_at: agora };
    const capitulos = [1, 2, 3].map((i) => ({
      id: `c${i}`, universe_id: 'u1', book_id: 'b1', title: `Capítulo ${i} — A chegada ao farol`,
      content: i === 1 ? '<p>Era uma noite fria em Hopi, e o vento batia nas janelas da casa velha.</p>' : '',
      summary: '', position: i, word_count: 12, created_at: agora, updated_at: agora,
    }));
    store.stories.set([historia]);
    store.books.set([livro]);
    store.universeBooks.set([livro]);
    store.chapters.set(capitulos);
    store.universeChapters.set(capitulos);
    store.activeStory.set(historia);
    store.activeBook.set(livro);
    store.activeChapter.set(capitulos[0]);
    store.editorTitle.set(capitulos[0].title);
    store.editorContent.set(capitulos[0].content);
    store.expandedStoryIds.set(new Set(['s1']));
    store.expandedBookIds.set(new Set(['b1']));
    window.ng.applyChanges(pagina);
  });
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

async function estadoDaNavegacao(page) {
  return page.evaluate(() => {
    const nav = window.ng.getComponent(document.querySelector('app-mobile-navigation'));
    return { aberto: nav.isOpen(), abertura: nav.open, posicao: nav.position };
  });
}

/** Toque real (eventos de toque do Chromium, que viram pointer events com pointerType "touch"). */
async function arrastarComDedo(page, de, para, passos = 12, intervalo = 16) {
  const cdp = await page.context().newCDPSession(page);
  const ponto = (x, y) => [{ x, y, id: 1, radiusX: 4, radiusY: 4, force: 1 }];
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: ponto(de.x, de.y) });
  for (let i = 1; i <= passos; i += 1) {
    const x = de.x + ((para.x - de.x) * i) / passos;
    const y = de.y + ((para.y - de.y) * i) / passos;
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: ponto(x, y) });
    await page.waitForTimeout(intervalo);
  }
  await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  await cdp.detach();
}

/** Espera as animações de entrada (folhas, diálogos) terminarem antes de medir. */
async function semAnimacao(locator) {
  await locator.evaluate((el) => Promise.all(el.getAnimations({ subtree: true }).map((a) => a.finished.catch(() => undefined))));
}

/** Espera o navegador assentar: aberto ou fechado, e a roda num cartão inteiro. */
async function navegacaoAssentada(page) {
  await expect.poll(async () => {
    const estado = await estadoDaNavegacao(page);
    return Number.isInteger(estado.posicao);
  }).toBe(true);
}

async function centroDaAlca(page) {
  const caixa = await page.locator('.nh-mnav-handle').boundingBox();
  return { x: caixa.x + caixa.width - 12, y: caixa.y + caixa.height / 2 };
}

// ── celular ───────────────────────────────────────────────────────────────────

test.describe('celular Android', () => {
  test.beforeEach(({}, testInfo) => test.skip(ehDesktop(testInfo), 'só celular'));

  test('shell mobile: sem barra de título do desktop, sem rolagem lateral, alça com alvo de polegar', async ({ page }) => {
    await abrirBiblioteca(page);
    await expect(page.locator('html')).toHaveClass(/nh-mobile/u);
    await expect(page.locator('app-mobile-shell')).toBeVisible();
    await expect(page.locator('app-titlebar')).toHaveCount(0);
    await expect(page.locator('app-mobile-topbar')).toBeVisible();
    const alca = await page.locator('.nh-mnav-handle').boundingBox();
    expect(alca.width).toBeGreaterThanOrEqual(44);
    expect(alca.height).toBeGreaterThanOrEqual(44);
    await semRolagemHorizontal(page, 'biblioteca');
  });

  test('toque na alça abre, empurrar para a direita fecha', async ({ page }) => {
    await abrirBiblioteca(page);
    const alca = await centroDaAlca(page);
    await page.touchscreen.tap(alca.x, alca.y);
    await expect.poll(async () => (await estadoDaNavegacao(page)).aberto).toBe(true);
    await expect(page.locator('.nh-mnav-card').first()).toBeVisible();
    await expect.poll(async () => (await estadoDaNavegacao(page)).abertura).toBe(1);

    const tela = page.viewportSize();
    await arrastarComDedo(page, { x: tela.width * 0.25, y: tela.height * 0.5 }, { x: tela.width * 0.95, y: tela.height * 0.5 });
    await expect.poll(async () => (await estadoDaNavegacao(page)).aberto).toBe(false);
  });

  test('arrastar da borda abre, e a roda vertical troca o cartão da frente', async ({ page }) => {
    await abrirBiblioteca(page);
    const alca = await centroDaAlca(page);
    const tela = page.viewportSize();
    await arrastarComDedo(page, alca, { x: tela.width * 0.35, y: alca.y }, 16, 20);
    await expect.poll(async () => (await estadoDaNavegacao(page)).aberto).toBe(true);
    await page.waitForTimeout(500);

    await navegacaoAssentada(page);
    const antes = (await estadoDaNavegacao(page)).posicao;
    await arrastarComDedo(page, { x: tela.width / 2, y: tela.height * 0.7 }, { x: tela.width / 2, y: tela.height * 0.35 }, 14, 22);
    await expect.poll(async () => Math.round((await estadoDaNavegacao(page)).posicao)).toBeGreaterThan(Math.round(antes));
    await navegacaoAssentada(page);
  });

  test('peteleco curto e rápido na alça abre', async ({ page }) => {
    await abrirBiblioteca(page);
    const alca = await centroDaAlca(page);
    // Curto: 70px, abaixo do limiar de distância que abriria mesmo devagar. Rápido: dois
    // movimentos sem pausa (o protocolo de teste já leva ~40ms por evento).
    await arrastarComDedo(page, alca, { x: alca.x - 70, y: alca.y }, 2, 0);
    await expect.poll(async () => (await estadoDaNavegacao(page)).aberto).toBe(true);
  });

  test('dica "Puxe para navegar" aparece na primeira vez e não na segunda', async ({ page }) => {
    await page.addInitScript(() => sessionStorage.setItem('nh-e2e-dica', '1'));
    await page.goto('/');
    await page.evaluate(() => localStorage.removeItem('narrahub.mobileNavigationHintSeen'));
    await page.reload();
    await expect(page.locator('.nh-mnav-coach')).toContainText('Puxe para navegar');
    const alca = await centroDaAlca(page);
    await page.touchscreen.tap(alca.x, alca.y);
    await expect(page.locator('.nh-mnav-coach')).toHaveCount(0);
    await page.reload();
    await page.waitForFunction(() => Boolean(document.querySelector('app-mobile-navigation')));
    await page.waitForTimeout(800);
    await expect(page.locator('.nh-mnav-coach')).toHaveCount(0);
  });

  test('universo: sem sidebar e sem cabeçalho de desktop; ações no "•••"', async ({ page }) => {
    await abrirEscrita(page);
    await expect(page.locator('app-universe-sidebar')).toHaveCount(0);
    await expect(page.locator('.workspace-header')).toHaveCount(0);
    await page.getByRole('button', { name: 'Mais ações' }).click();
    const folha = page.locator('app-mobile-sheet .nh-sheet');
    await expect(folha).toBeVisible();
    await semAnimacao(folha);
    for (const acao of ['Renomear universo', 'Tags do universo', 'Compartilhar']) {
      await expect(folha.getByRole('button', { name: acao })).toBeVisible();
    }
    const caixa = await folha.boundingBox();
    expect(caixa.y + caixa.height).toBeLessThanOrEqual(page.viewportSize().height + 1);
  });

  test('escrita: editor com a largura da tela, ferramentas rolam por dentro, folhas cabem', async ({ page }) => {
    await abrirEscrita(page);
    const tela = page.viewportSize();
    await expect(page.locator('.ProseMirror')).toBeVisible();
    const editor = await page.locator('.nh-document').boundingBox();
    expect(editor.width).toBeLessThanOrEqual(tela.width);
    const ferramentas = await page.locator('.nh-editor-toolbar').evaluate((el) => ({ largura: el.clientWidth, conteudo: el.scrollWidth }));
    expect(ferramentas.largura).toBeLessThanOrEqual(tela.width);
    await expect(page.locator('.view-tools')).toHaveCount(0);
    await semRolagemHorizontal(page, 'escrita');

    await page.getByRole('button', { name: /Capítulos/u }).click();
    const arvore = page.locator('app-mobile-sheet .nh-sheet');
    await expect(arvore).toBeVisible();
    await semAnimacao(arvore);
    await expect(arvore.getByText('Capítulo 2 — A chegada ao farol')).toBeVisible();
    const caixa = await arvore.boundingBox();
    expect(caixa.y).toBeGreaterThanOrEqual(0);
    expect(caixa.y + caixa.height).toBeLessThanOrEqual(tela.height + 1);
    // Cada item tem um "⋯" de dedo, no lugar das ações que dependem de hover.
    const mais = await arvore.locator('.tree-more').first().boundingBox();
    expect(mais.width).toBeGreaterThanOrEqual(44);
    await semRolagemHorizontal(page, 'folha de capítulos');
  });

  test('diálogo vira folha e continua inteiro com o teclado aberto', async ({ page }) => {
    await abrirEscrita(page);
    await page.evaluate(() => {
      const pagina = window.ng.getComponent(document.querySelector('app-writing-page'));
      pagina.openCreateStory();
      window.ng.applyChanges(pagina);
    });
    const painel = page.locator('[data-nh-dialog-panel]');
    await expect(painel).toBeVisible();
    await semAnimacao(painel);
    const tela = page.viewportSize();
    let caixa = await painel.boundingBox();
    expect(caixa.width).toBeGreaterThanOrEqual(tela.width - 1);
    expect(caixa.y + caixa.height).toBeLessThanOrEqual(tela.height + 1);
    const fonte = await painel.locator('input').first().evaluate((el) => getComputedStyle(el).fontSize);
    expect(fonte).toBe('16px');

    // O Android encolhe a WebView quando o teclado abre (MainActivity). Aqui: a tela perde 45%.
    const comTeclado = { width: tela.width, height: Math.round(tela.height * 0.55) };
    await page.setViewportSize(comTeclado);
    await expect.poll(async () => Math.round((await page.locator('app-mobile-shell').boundingBox()).height)).toBe(comTeclado.height);
    caixa = await painel.boundingBox();
    expect(caixa.y).toBeGreaterThanOrEqual(0);
    expect(caixa.y + caixa.height).toBeLessThanOrEqual(comTeclado.height + 1);
    const salvar = await painel.locator('footer button').last().boundingBox();
    expect(salvar.y + salvar.height, 'o botão de salvar ficou atrás do teclado').toBeLessThanOrEqual(comTeclado.height + 1);
  });

  test('nenhuma área produz rolagem horizontal da página', async ({ page }) => {
    await abrirEscrita(page);
    for (const destino of ['planejamento', 'timeline', 'configuracoes', 'inicio']) {
      await page.evaluate(async (id) => {
        await window.ng.getComponent(document.querySelector('app-root-layout')).navigateFromMobile(id);
      }, destino);
      await page.waitForTimeout(900);
      await semRolagemHorizontal(page, destino);
    }
  });
});

// ── desktop ───────────────────────────────────────────────────────────────────

test.describe('desktop', () => {
  test.beforeEach(({}, testInfo) => test.skip(!ehDesktop(testInfo), 'só desktop'));

  test('desktop intacto: barra de título, sidebar e cabeçalho; nada do mobile', async ({ page }) => {
    await abrirEscrita(page);
    await expect(page.locator('html')).not.toHaveClass(/nh-mobile/u);
    await expect(page.locator('app-titlebar')).toBeVisible();
    await expect(page.locator('app-mobile-shell')).toHaveCount(0);
    await expect(page.locator('app-mobile-navigation')).toHaveCount(0);
    await expect(page.locator('app-universe-sidebar')).toHaveCount(1);
    await expect(page.locator('.workspace-header')).toBeVisible();
    await expect(page.locator('aside.project-tree')).toBeVisible();
    await expect(page.locator('.view-tools')).toBeVisible();
  });

  test('janela estreita com mouse continua sendo desktop', async ({ page }) => {
    await page.setViewportSize({ width: 700, height: 900 });
    await abrirBiblioteca(page);
    await expect(page.locator('html')).not.toHaveClass(/nh-mobile/u);
    await expect(page.locator('app-mobile-shell')).toHaveCount(0);
  });
});
