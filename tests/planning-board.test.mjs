import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
  parsePlanningFieldValues,
  reorderPlanningItems,
  serializePlanningFieldValues,
} from '../src/app/features/planning/planning-board.utils.ts';

function card(id, status, sortOrder) {
  return {
    id,
    universe_id: 'u1',
    chapter_id: null,
    title: id,
    description: '',
    image: '',
    custom_field_values: '{}',
    status,
    target_words: 0,
    sort_order: sortOrder,
    created_at: '',
    updated_at: '',
  };
}

test('move um card entre etapas e recalcula a ordem das duas colunas', () => {
  const items = [card('a', 'IDEIAS', 0), card('b', 'IDEIAS', 1), card('c', 'PLANEJADO', 0)];
  const result = reorderPlanningItems(items, 'b', 'PLANEJADO', 0);

  assert.deepEqual(result.filter((item) => item.status === 'IDEIAS').map(({ id, sort_order }) => [id, sort_order]), [['a', 0]]);
  assert.deepEqual(result.filter((item) => item.status === 'PLANEJADO').map(({ id, sort_order }) => [id, sort_order]), [['b', 0], ['c', 1]]);
});

test('reordena um card dentro da mesma etapa sem duplicá-lo', () => {
  const items = [card('a', 'IDEIAS', 0), card('b', 'IDEIAS', 1), card('c', 'IDEIAS', 2)];
  const result = reorderPlanningItems(items, 'c', 'IDEIAS', 0);

  assert.deepEqual(result.map(({ id, sort_order }) => [id, sort_order]), [['c', 0], ['a', 1], ['b', 2]]);
  assert.equal(new Set(result.map((item) => item.id)).size, 3);
});

test('valores personalizados inválidos são descartados na leitura e escrita', () => {
  const parsed = parsePlanningFieldValues('{"text":"ok","flag":true,"links":["s1"],"bad":{"nested":true}}');
  assert.deepEqual(parsed, { text: 'ok', flag: true, links: ['s1'] });
  assert.deepEqual(JSON.parse(serializePlanningFieldValues({ ...parsed, empty: null })), parsed);
  assert.deepEqual(parsePlanningFieldValues('not-json'), {});
});

test('ficha CRM mede o workspace e mantém cabeçalho e rodapé fora da rolagem', () => {
  const css = readFileSync(
    new URL('../src/app/features/planning/planning-board.component.css', import.meta.url),
    'utf8',
  );

  assert.match(css, /\.crm-modal\s*\{[^}]*width:min\(1040px,calc\(100% - 8px\)\)/u);
  assert.match(css, /\.crm-modal\s*\{[^}]*grid-template-rows:auto minmax\(0,1fr\) auto/u);
  assert.match(css, /\.crm-body\s*\{[^}]*overflow-y:auto/u);
  assert.doesNotMatch(css, /\.crm-modal\s*\{[^}]*(?:100vw|100vh)/u);
});
