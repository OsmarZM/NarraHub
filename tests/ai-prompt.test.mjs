import assert from 'node:assert/strict';
import test from 'node:test';
import {
  buildAiMessages,
  compactAiContext,
  contextBudgetFor,
  isAiEcho,
  sanitizeAiCompletion,
} from '../src/app/core/ai/ai-prompt.ts';

test('compacta contexto local preservando identidade e trecho prioritário', () => {
  const context = `UNIVERSO: Aether\n${'cânone intermediário '.repeat(700)}\nTEXTO SELECIONADO: Lia fechou a porta.`;
  const budget = contextBudgetFor('local');
  const compacted = compactAiContext(context, budget);

  assert.ok(compacted.length <= budget);
  assert.match(compacted, /^UNIVERSO: Aether/u);
  assert.match(compacted, /Contexto intermediário omitido/u);
  assert.match(compacted, /TEXTO SELECIONADO: Lia fechou a porta\.$/u);
});

test('preserva a tarefa antes do contexto compacto e ativa no_think no modo local', () => {
  const messages = buildAiMessages('Reescreva o trecho.', 'TEXTO: Era noite.', 'local');
  const user = messages[1].content;

  assert.ok(user.indexOf('TAREFA') < user.indexOf('CONTEXTO SELECIONADO'));
  assert.match(user, /Reescreva o trecho\./u);
  assert.match(messages[0].content, /^\/no_think/u);
});

test('detecta eco da instrução e de uma transformação obrigatória', () => {
  assert.equal(isAiEcho('Reescreva este texto.', 'Reescreva este texto.'), true);
  assert.equal(isAiEcho('TAREFA A EXECUTAR\nReescreva', 'Reescreva'), true);
  assert.equal(isAiEcho('A porta rangeu.', 'Reescreva.', {
    sourceText: 'A porta rangeu.',
    requireTransformation: true,
  }), true);
});

test('não rejeita uma correção que realmente modificou o texto', () => {
  assert.equal(isAiEcho('O usuário não conseguiu entrar.', 'Corrija a ortografia.', {
    sourceText: 'o usuario nao conseguio entrar',
    requireTransformation: false,
  }), false);
});

test('remove raciocínio e prefixos sem apagar a resposta editorial', () => {
  const result = sanitizeAiCompletion('<think>análise interna</think>\nResultado: A chuva cessou.');
  assert.equal(result, 'A chuva cessou.');
});
