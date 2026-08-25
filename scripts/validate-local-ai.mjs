import assert from 'node:assert/strict';
import {
  buildAiMessages,
  isAiEcho,
  sanitizeAiCompletion,
} from '../src/app/core/ai/ai-prompt.ts';

const endpoint = (process.env.NARRAHUB_AI_ENDPOINT || 'http://127.0.0.1:11439/v1').replace(/\/+$/u, '');
const model = process.env.NARRAHUB_AI_MODEL || 'narrahub-local';

const cases = [
  {
    name: 'correcao',
    instruction: 'Corrija ortografia, gramática e pontuação. Entregue somente o texto corrigido.',
    context: 'TEXTO SELECIONADO: com certeza o usuario nao conseguio entrar na sala',
    sourceText: 'com certeza o usuario nao conseguio entrar na sala',
    requireTransformation: true,
    verify: (text) => /usuário/iu.test(text) && /conseguiu/iu.test(text),
  },
  {
    name: 'reescrita',
    instruction: 'Reescreva como prosa narrativa mais fluida, sem repetir literalmente o original.',
    context: 'TEXTO SELECIONADO: Osmar entrou na sala. Osmar entrou e olhou para a janela.',
    sourceText: 'Osmar entrou na sala. Osmar entrou e olhou para a janela.',
    requireTransformation: true,
    verify: (text) => /Osmar/iu.test(text) && /janela/iu.test(text),
  },
  {
    name: 'contexto-longo',
    instruction: 'Gere apenas um nome original para uma cidade de fantasia sombria. Não explique.',
    context: `UNIVERSO: Aether\n${'CÂNONE INTERMEDIÁRIO: ruínas, névoa e conflitos antigos. '.repeat(450)}\nFOCO ATUAL: cidade portuária construída sobre ossos de criaturas marinhas.`,
    verify: (text) => text.length >= 2 && text.length <= 120 && !/contexto|cânone/iu.test(text),
  },
];

async function request(item, retryAfterEcho = false) {
  const response = await fetch(`${endpoint}/chat/completions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model,
      temperature: 0.7,
      top_p: 0.8,
      top_k: 20,
      min_p: 0,
      max_tokens: 240,
      stream: false,
      messages: buildAiMessages(item.instruction, item.context, 'local', retryAfterEcho),
    }),
  });
  assert.equal(response.ok, true, `${item.name}: servidor respondeu HTTP ${response.status}`);
  const payload = await response.json();
  const content = sanitizeAiCompletion(payload.choices?.[0]?.message?.content || '');
  return { content, payload };
}

for (const item of cases) {
  let { content, payload } = await request(item);
  assert.ok(content, `${item.name}: resposta vazia`);
  if (isAiEcho(content, item.instruction, item)) ({ content, payload } = await request(item, true));
  assert.equal(isAiEcho(content, item.instruction, item), false, `${item.name}: resposta foi eco após retry`);
  assert.equal(item.verify(content), true, `${item.name}: resposta não cumpriu o contrato editorial: ${JSON.stringify(content)}`);
  assert.ok((payload.usage?.prompt_tokens ?? 0) < 2_000, `${item.name}: prompt ultrapassou o orçamento local`);
  process.stdout.write(`${item.name}: ok (${payload.usage?.prompt_tokens ?? '?'} + ${payload.usage?.completion_tokens ?? '?'} tokens)\n`);
}

process.stdout.write('IA local validada com geração real.\n');
