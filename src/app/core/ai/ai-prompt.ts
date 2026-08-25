export interface AiPromptOptions {
  sourceText?: string;
  requireTransformation?: boolean;
  maxTokens?: number;
}

export interface AiPromptMessage {
  role: 'system' | 'user';
  content: string;
}

const LOCAL_CONTEXT_CHAR_BUDGET = 4_500;
const CUSTOM_CONTEXT_CHAR_BUDGET = 18_000;
const CONTEXT_HEAD_CHARS = 1_200;

export function contextBudgetFor(mode: 'local' | 'custom'): number {
  return mode === 'local' ? LOCAL_CONTEXT_CHAR_BUDGET : CUSTOM_CONTEXT_CHAR_BUDGET;
}

export function compactAiContext(context: string, maxChars: number): string {
  const normalized = context.trim();
  if (normalized.length <= maxChars) return normalized;

  const marker = '\n\n[Contexto intermediário omitido pelo NarraHub para preservar a tarefa e o trecho em edição.]\n\n';
  const headSize = Math.min(CONTEXT_HEAD_CHARS, Math.floor(maxChars * 0.3));
  const tailSize = Math.max(0, maxChars - headSize - marker.length);
  return `${normalized.slice(0, headSize).trimEnd()}${marker}${normalized.slice(-tailSize).trimStart()}`;
}

export function buildAiMessages(
  instruction: string,
  context: string,
  mode: 'local' | 'custom',
  retryAfterEcho = false,
): AiPromptMessage[] {
  const compactContext = compactAiContext(context, contextBudgetFor(mode));
  const retryRule = retryAfterEcho
    ? '\nA resposta anterior apenas repetiu a solicitação ou o original. Nesta tentativa, execute de fato a transformação e produza um resultado novo.'
    : '';

  return [
    {
      role: 'system',
      content: `${mode === 'local' ? '/no_think\n' : ''}Você é um assistente editorial. Preserve a voz do escritor, não invente fatos fora do contexto e responda em português do Brasil. Retorne apenas o texto útil para a tarefa.`,
    },
    {
      role: 'user',
      content: [
        'TAREFA',
        `${instruction.trim()}${retryRule}`,
        '',
        'CONTEXTO SELECIONADO PELO NARRAHUB',
        compactContext || 'Nenhum contexto adicional foi fornecido.',
      ].join('\n'),
    },
  ];
}

export function sanitizeAiCompletion(content: string): string {
  return content
    .replace(/<think>[\s\S]*?<\/think>/giu, '')
    .replace(/^\s*(?:resultado|resposta|contexto|texto(?:\s+(?:final|reescrito|corrigido))?)\s*:?\s*/iu, '')
    .trim();
}

export function isAiEcho(output: string, instruction: string, options: AiPromptOptions = {}): boolean {
  const normalizedOutput = normalizeComparison(output);
  const normalizedInstruction = normalizeComparison(instruction);
  if (!normalizedOutput) return false;
  if (normalizedOutput.includes('contexto selecionado pelo narrahub')
      || normalizedOutput.includes('tarefa a executar')
      || normalizedOutput.includes('a resposta anterior apenas repetiu')) return true;
  if (normalizedOutput === normalizedInstruction) return true;
  if (normalizedInstruction.length >= 20
      && normalizedOutput.startsWith(normalizedInstruction)
      && normalizedOutput.length <= normalizedInstruction.length + 40) return true;

  if (options.requireTransformation && options.sourceText) {
    return normalizedOutput === normalizeComparison(options.sourceText);
  }
  return false;
}

function normalizeComparison(value: string): string {
  return value
    .trim()
    .replace(/^["“”'‘’]+|["“”'‘’]+$/gu, '')
    .replace(/\s+/gu, ' ')
    .toLocaleLowerCase('pt-BR');
}
