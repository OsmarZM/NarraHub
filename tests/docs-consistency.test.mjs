import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { execSync } from 'node:child_process';

/**
 * A documentação de estado tem que descrever a `main` atual, não uma foto antiga.
 *
 * Isto importa mais neste projeto do que num comum: `PROJECT_STATE.md` e `ARCHITECTURE.md`
 * são a memória compartilhada de três agentes. Quando ela envelhece, os três passam a
 * raciocinar em cima do mesmo erro — e com confiança, porque o documento se declara fonte da
 * verdade.
 *
 * Já aconteceu duas vezes: o arquivo anunciou 0.9.1 depois da 0.9.2 sair, e descreveu
 * `commands/` e o `WorkspaceLayout` como pendências semanas depois de resolvidos, ao mesmo
 * tempo em que outra linha do MESMO arquivo dizia o contrário.
 *
 * Estes testes não conferem prosa — isso não dá para automatizar. Eles conferem as poucas
 * afirmações que têm contraparte no disco.
 */

const raiz = fileURLToPath(new URL('../', import.meta.url));
const ler = (relativo) => readFileSync(new URL(relativo, import.meta.url), 'utf8');

const DOCUMENTOS_DE_ESTADO = [
  '../docs/ai/PROJECT_STATE.md',
  '../docs/ARCHITECTURE.md',
  '../AGENTS.md',
];

test('nenhum documento descreve o commands/ legado como existente', () => {
  // O diretório foi removido na Fase 3. Enquanto a documentação dizia "ainda presente", um
  // agente podia abrir a tarefa de removê-lo e não achar nada — ou pior, recriá-lo.
  if (existsSync(new URL('../src-tauri/src/commands', import.meta.url))) return;

  const frasesQueViraramMentira = [
    'ainda presente',
    'coexiste com `interface/tauri/`',
    'dois caminhos até o banco',
    'dois caminhos completos',
    'resta limpar',
  ];
  for (const relativo of DOCUMENTOS_DE_ESTADO) {
    const texto = ler(relativo);
    for (const frase of frasesQueViraramMentira) {
      const linhas = texto
        .split('\n')
        .filter((linha) => linha.includes(frase) && linha.includes('commands'));
      assert.deepEqual(
        linhas,
        [],
        `${relativo} descreve src-tauri/src/commands/ como existente, e ele foi removido na `
          + `Fase 3:\n  ${linhas.join('\n  ')}`,
      );
    }
  }
});

test('a fase ativa não aparece na lista de "não trabalhar ainda"', () => {
  // A contradição mais cara possível neste arquivo: dizer que a fase é a ativa e, algumas
  // linhas abaixo, mandar não trabalhar nela. Um agente que leia de cima para baixo obedece
  // a última instrução que viu.
  const estado = ler('../docs/ai/PROJECT_STATE.md');

  const faseAtiva = estado.match(/## Fase ativa\s*```text\s*(.+?)\s*```/su)?.[1]?.trim();
  assert.ok(faseAtiva, 'PROJECT_STATE.md precisa declarar a fase ativa num bloco de código');

  // "FASE 4 — Sync V2" → "Sync V2"
  const assunto = faseAtiva.split('—').at(-1)?.trim();
  assert.ok(assunto, `não consegui extrair o assunto da fase ativa de: ${faseAtiva}`);

  const naoTrabalhar = estado.match(/## Não trabalhar ainda\s*```text\s*(.+?)\s*```/su)?.[1] ?? '';
  assert.ok(
    !naoTrabalhar.toLowerCase().includes(assunto.toLowerCase()),
    `"${assunto}" é a fase ativa e ainda está na lista de "não trabalhar ainda". `
      + 'Uma das duas afirmações está errada, e um agente vai obedecer a última que ler.',
  );
});

test('a versão corrente do PROJECT_STATE bate com o manifesto', () => {
  // Duplica de propósito o que `release:validate-version` já checa. Aqui o teste roda em
  // `npm run test:architecture`, que é o gate de todo PR; lá roda no fluxo de release. A
  // memória compartilhada desatualizada é barata de detectar e cara de descobrir tarde.
  const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  const estado = ler('../docs/ai/PROJECT_STATE.md');
  const declarada = estado.match(/^\|\s*Versão corrente\s*\|\s*\*{0,2}(\S+?)\*{0,2}\s*\|/mu)?.[1];

  assert.equal(
    declarada,
    pkg.version,
    'PROJECT_STATE.md declara uma versão diferente da do package.json',
  );
});

test('todo ADR citado na documentação de estado existe', () => {
  // Referência a um ADR que não existe manda o leitor procurar uma decisão que nunca foi
  // escrita — e ele volta achando que a decisão é dele.
  const citados = new Set();
  for (const relativo of [...DOCUMENTOS_DE_ESTADO, '../docs/ai/ROADMAP.md', '../TASKS.md']) {
    for (const [, numero] of ler(relativo).matchAll(/ADR[\s-]?(\d{4})/gu)) {
      citados.add(numero);
    }
  }
  // Salvaguarda modesta, e vale ser honesto sobre o limite dela: ela pega uma varredura
  // totalmente quebrada, não uma parcialmente quebrada. A primeira versão exigia mais de
  // três ADRs citados — número que eu inventei sem contar, e que reprovou por si só quando
  // a documentação de estado cita dois.
  assert.ok(citados.size >= 1, 'a varredura de ADRs citados não encontrou nenhum; ela quebrou');

  const indice = ler('../docs/ADR/README.md');
  const ausentes = [...citados].filter((numero) => !indice.includes(numero));
  assert.deepEqual(
    ausentes,
    [],
    `ADR citado na documentação e ausente do índice em docs/ADR/README.md: ${ausentes.join(', ')}`,
  );
});

/**
 * Caractere de controle literal em documento — a terceira aparicao do mesmo bug.
 *
 * Escrever `\b` numa expressao regular atravessando camadas de escape (shell, Python, JSON)
 * produz o byte 0x08 em vez das duas letras. A regex passa a casar outra coisa e continua
 * parecendo certa na tela, porque o terminal nao desenha um backspace. Nesta sessao isso
 * corrompeu cinco asserçoes de teste — tres das quais eu ja tinha declarado verificadas — e
 * depois voltou dentro da propria frase do TASKS.md que descrevia o episodio.
 *
 * O byte e invisivel; o gate nao precisa ser inteligente, precisa existir. Nenhum documento
 * deste repositorio tem motivo para conter controle fora de tabulacao e quebra de linha.
 */
test('nenhum documento carrega caractere de controle invisivel', () => {
  const arquivos = execSync('git ls-files "*.md" "*.mjs" "*.rs" "*.ts" "*.sql"', {
    cwd: raiz,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
  })
    .split('\n')
    .filter(Boolean);

  // Tabulacao (0x09), quebra de linha (0x0A) e retorno (0x0D) sao legitimos. O resto nao.
  const proibidos = /[\u0000-\u0008\u000B\u000C\u000E-\u001F]/u;

  const sujos = [];
  for (const arquivo of arquivos) {
    const conteudo = readFileSync(new URL(`../${arquivo}`, import.meta.url), 'utf8');
    const posicao = conteudo.search(proibidos);
    if (posicao === -1) continue;
    const linha = conteudo.slice(0, posicao).split('\n').length;
    const codigo = conteudo.codePointAt(posicao).toString(16).padStart(4, '0');
    sujos.push(`${arquivo}:${linha} (U+${codigo.toUpperCase()})`);
  }

  assert.deepEqual(
    sujos,
    [],
    `Caractere de controle literal encontrado. Quase sempre e uma sequencia de escape que ` +
      `atravessou uma camada a mais e virou o byte de verdade — \\b vira 0x08, \\f vira 0x0C. ` +
      `Escreva o arquivo por um script em disco, sem passar por aspas de shell: ${sujos.join(', ')}`,
  );
});
