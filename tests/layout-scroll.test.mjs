import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

/**
 * Uma superfície que ocupa a altura toda da janela e não rola é conteúdo inalcançável.
 *
 * Foi assim que a lista de backups dos Ajustes sumiu em 1366×768: `root-layout.component.css`
 * tem `ViewEncapsulation.None`, e duas regras globais competiam pelo mesmo seletor —
 *
 *   .home-view,.feature-page { height: 100%; overflow-y: auto; ... }
 *   .feature-page            { ...; overflow: hidden; }
 *
 * A segunda vem depois, com a mesma especificidade, e o atalho `overflow` zera o eixo
 * vertical junto com o horizontal. O resultado é uma página da altura exata da janela que
 * corta tudo o que passar disso, sem barra de rolagem. Em telas grandes nada passava; em
 * 1366×768, passava.
 *
 * O teste resolve a cascata do jeito que o navegador resolve: última declaração vence.
 */

const CSS = new URL('../src/app/root-layout.component.css', import.meta.url);

/** Declarações de um seletor, na ordem em que aparecem no arquivo. */
function declaracoesDe(css, seletorAlvo) {
  const encontradas = [];
  // Sem isto, um comentário logo acima de uma regra entra no seletor capturado e a regra
  // some da análise — foi exatamente o que aconteceu na primeira versão deste teste, que
  // passava mesmo com o defeito reintroduzido de propósito.
  const semComentarios = css.replace(/\/\*[\s\S]*?\*\//gu, '');
  const regra = /([^{}]+)\{([^{}]*)\}/gu;
  let match;
  while ((match = regra.exec(semComentarios)) !== null) {
    const seletores = match[1].split(',').map((s) => s.trim());
    if (!seletores.includes(seletorAlvo)) continue;
    for (const declaracao of match[2].split(';')) {
      const [propriedade, ...resto] = declaracao.split(':');
      if (!resto.length) continue;
      encontradas.push({ propriedade: propriedade.trim(), valor: resto.join(':').trim() });
    }
  }
  return encontradas;
}

/** Valor efetivo de `overflow-y`, considerando que o atalho `overflow` sobrescreve os eixos. */
function overflowVerticalEfetivo(declaracoes) {
  let atual = 'visible';
  for (const { propriedade, valor } of declaracoes) {
    if (propriedade === 'overflow-y') atual = valor;
    else if (propriedade === 'overflow') atual = valor.trim().split(/\s+/u).at(-1);
  }
  return atual;
}

function alturaTravada(declaracoes) {
  return declaracoes.some(
    ({ propriedade, valor }) => propriedade === 'height' && /100%|100vh/u.test(valor),
  );
}

// Só superfícies de página, que carregam o conteúdo. Contêineres do shell — `.content-stage`,
// `.nh-content` — cortam de propósito e delegam a rolagem para a página lá dentro; para eles
// `overflow: hidden` é o desenho correto, não o defeito.
for (const seletor of ['.feature-page', '.home-view']) {
  test(`${seletor} não pode ocupar a janela inteira e ainda cortar sem rolar`, () => {
    const css = readFileSync(CSS, 'utf8');
    const declaracoes = declaracoesDe(css, seletor);
    if (declaracoes.length === 0) return; // seletor removido: nada a proteger

    if (!alturaTravada(declaracoes)) return; // altura livre cresce e o pai rola

    assert.notEqual(
      overflowVerticalEfetivo(declaracoes),
      'hidden',
      `${seletor} tem altura travada na janela e overflow vertical hidden: o conteúdo que ` +
        'passar da tela fica inalcançável. Se a intenção era só cortar na horizontal, use ' +
        '`overflow-x: hidden` em vez do atalho `overflow`.',
    );
  });
}
