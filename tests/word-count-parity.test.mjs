// A contagem de palavras sai do payload canônico do capítulo (Sync V2, B2): quem recebe recalcula
// em Rust (`sync_codec/palavras.rs`). Este teste prende os dois lados à mesma regra:
//  - a regex dos dois `countWords` do frontend continua a mesma;
//  - os vetores da fixture, que o Rust também confere, dão o mesmo número com essa regex.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const REGRA = "const normalized = content.replace(/<[^>]+>/g, ' ').trim();\n    return normalized ? normalized.split(/\\s+/u).length : 0;";

function countWords(content) {
  const normalized = content.replace(/<[^>]+>/g, ' ').trim();
  return normalized ? normalized.split(/\s+/u).length : 0;
}

test('os dois countWords do frontend usam a regra que o Rust reproduz', () => {
  for (const arquivo of [
    'src/app/features/manuscript/state/manuscript.store.ts',
    'src/app/features/manuscript/writing-page.component.ts',
  ]) {
    const fonte = readFileSync(arquivo, 'utf8').replace(/\r\n/g, '\n');
    assert.ok(fonte.includes(REGRA), `${arquivo} mudou a contagem de palavras; atualize palavras.rs e a fixture`);
  }
});

test('os vetores compartilhados dão a mesma contagem no JavaScript', () => {
  const vetores = JSON.parse(readFileSync('src-tauri/fixtures/contagem_de_palavras.json', 'utf8'));
  assert.ok(vetores.length >= 10);
  for (const { content, palavras } of vetores) {
    assert.equal(countWords(content), palavras, JSON.stringify(content));
  }
});
