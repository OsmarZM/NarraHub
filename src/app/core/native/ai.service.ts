import { Injectable, computed, signal } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { AiPromptOptions, buildAiMessages, isAiEcho, sanitizeAiCompletion } from '../ai/ai-prompt';

export type AiMode = 'off' | 'local' | 'custom';
export type LocalAiState = 'checking' | 'unsupported' | 'not_installed' | 'installing' | 'stopped' | 'starting' | 'ready' | 'error';

export interface AiSettings {
  mode: AiMode;
  endpoint: string;
  model: string;
}

export interface AiHardwareProfile {
  logicalCores: number;
  physicalCores: number;
  totalMemoryGb: number;
  availableMemoryGb: number;
  availableStorageGb: number;
  cpu: string;
  architecture: string;
  avx2: boolean;
  gpu: string | null;
  gpuMemoryGb: number | null;
  score: number;
}

export interface AiModelProfile {
  id: 'lite' | 'standard' | 'advanced';
  name: string;
  description: string;
  downloadBytes: number;
  minimumMemoryGb: number;
  useCases: string[];
}

export interface LocalAiStatus {
  state: LocalAiState;
  installed: boolean;
  running: boolean;
  supported: boolean;
  message: string;
  installedProfile: string | null;
  modelAlias: string | null;
  hardware: AiHardwareProfile;
  recommended: AiModelProfile;
  profiles: AiModelProfile[];
}

export interface LocalAiInstallProgress {
  stage: 'runtime' | 'model' | 'complete';
  percent: number;
  downloadedBytes: number;
  totalBytes: number;
  message: string;
}

export interface AiCreativeMemory {
  id: string;
  scope: string;
  kind: 'writing' | 'entity' | 'preference';
  summary: string;
  createdAt: string;
}

interface ChatCompletionResponse {
  choices?: Array<{ message?: { content?: string } }>;
  error?: { message?: string };
}

interface LocalEngine {
  endpoint: string;
  model: string;
}

const SETTINGS_KEY = 'narrahub.ai.settings';
const SESSION_API_KEY = 'narrahub.ai.sessionKey';
const WRITER_GUIDANCE_KEY = 'narrahub.ai.writerGuidance';
const CREATIVE_MEMORY_KEY = 'narrahub.ai.creativeMemory';
const MANAGED_ENDPOINT = 'http://127.0.0.1:11439/v1';
const FALLBACK_PROFILES: AiModelProfile[] = [
  { id: 'lite', name: 'NarraAI Lite', description: 'Mais leve para tarefas rápidas.', downloadBytes: 428_970_080, minimumMemoryGb: 4, useCases: ['nomes', 'tags', 'resumos curtos'] },
  { id: 'standard', name: 'NarraAI Standard', description: 'Equilíbrio para escrita diária.', downloadBytes: 1_282_439_264, minimumMemoryGb: 8, useCases: ['reescrita', 'diálogos', 'resumos de capítulo'] },
  { id: 'advanced', name: 'NarraAI Advanced', description: 'Mais qualidade para análises narrativas.', downloadBytes: 2_497_280_256, minimumMemoryGb: 16, useCases: ['análise narrativa', 'brainstorming', 'capítulos longos'] },
];

function fallbackStatus(): LocalAiStatus {
  const cores = Math.max(1, navigator.hardwareConcurrency || 2);
  const profile = cores >= 12 ? FALLBACK_PROFILES[2] : cores >= 6 ? FALLBACK_PROFILES[1] : FALLBACK_PROFILES[0];
  return {
    state: 'checking', installed: false, running: false, supported: isTauri(), message: 'Analisando este dispositivo…',
    installedProfile: null, modelAlias: null,
    hardware: { logicalCores: cores, physicalCores: Math.max(1, Math.floor(cores / 2)), totalMemoryGb: 0, availableMemoryGb: 0, availableStorageGb: 0, cpu: '', architecture: '', avx2: false, gpu: null, gpuMemoryGb: null, score: 0 },
    recommended: profile, profiles: FALLBACK_PROFILES,
  };
}

@Injectable({ providedIn: 'root' })
export class AiService {
  readonly settings = signal<AiSettings>(this.loadSettings());
  readonly localStatus = signal<LocalAiStatus>(fallbackStatus());
  readonly installProgress = signal<LocalAiInstallProgress | null>(null);
  readonly writerGuidance = signal(localStorage.getItem(WRITER_GUIDANCE_KEY) || '');
  readonly creativeMemory = signal<AiCreativeMemory[]>(this.loadCreativeMemory());
  readonly enabled = computed(() => {
    const settings = this.settings();
    return settings.mode === 'custom' || (settings.mode === 'local' && this.localStatus().installed);
  });
  private unlistenProgress: (() => void) | null = null;

  get sessionApiKey(): string { return sessionStorage.getItem(SESSION_API_KEY) ?? ''; }

  async initialize(): Promise<void> {
    if (!isTauri()) {
      this.localStatus.update((status) => ({ ...status, state: 'unsupported', supported: false, message: 'A IA gerenciada é instalada pelo aplicativo desktop.' }));
      return;
    }
    const { listen } = await import('@tauri-apps/api/event');
    this.unlistenProgress = await listen<LocalAiInstallProgress>('local-ai-install-progress', ({ payload }) => {
      this.installProgress.set(payload);
      this.localStatus.update((status) => ({ ...status, state: payload.stage === 'complete' ? 'starting' : 'installing', message: payload.message }));
    });
    const status = await this.refreshLocalStatus();
    if (this.settings().mode === 'local' && status.installed) await this.startLocalEngine();
  }

  dispose(): void { this.unlistenProgress?.(); this.unlistenProgress = null; }

  configure(settings: AiSettings, apiKey: string): void {
    const normalized: AiSettings = {
      mode: settings.mode,
      endpoint: (settings.mode === 'local' ? '' : settings.endpoint).trim().replace(/\/+$/u, ''),
      model: settings.mode === 'local' ? '' : settings.model.trim(),
    };
    this.validateSettings(normalized);
    if (normalized.mode === 'local' && !this.localStatus().installed) throw new Error('Autorize a instalação da IA local antes de ativá-la.');
    this.persistSettings(normalized, apiKey);
  }

  disable(): void { this.persistSettings({ mode: 'off', endpoint: '', model: '' }, ''); }

  setWriterGuidance(value: string): void {
    const normalized = value.trim().slice(0, 2_000);
    if (normalized) localStorage.setItem(WRITER_GUIDANCE_KEY, normalized);
    else localStorage.removeItem(WRITER_GUIDANCE_KEY);
    this.writerGuidance.set(normalized);
  }

  remember(scope: string, kind: AiCreativeMemory['kind'], summary: string): void {
    const normalized = summary.replace(/\s+/gu, ' ').trim().slice(0, 500);
    if (!scope || !normalized) return;
    const next = [
      ...this.creativeMemory(),
      { id: crypto.randomUUID(), scope, kind, summary: normalized, createdAt: new Date().toISOString() },
    ].filter((item) => item.scope !== scope || Date.now() - Date.parse(item.createdAt) < 1000 * 60 * 60 * 24 * 180);
    const scoped = next.filter((item) => item.scope === scope).slice(-12);
    const other = next.filter((item) => item.scope !== scope);
    const compact = [...other, ...scoped].slice(-80);
    localStorage.setItem(CREATIVE_MEMORY_KEY, JSON.stringify(compact));
    this.creativeMemory.set(compact);
  }

  memoryContext(scope: string): string {
    const memories = this.creativeMemory().filter((item) => item.scope === scope).slice(-8);
    const guidance = this.writerGuidance();
    return [
      guidance ? `ORIENTAÇÕES CONFIRMADAS PELO ESCRITOR:\n${guidance}` : '',
      memories.length ? `DECISÕES RECENTES ACEITAS:\n${memories.map((item) => `- ${item.summary}`).join('\n')}` : '',
    ].filter(Boolean).join('\n\n');
  }

  forgetCreativeMemory(scope?: string): void {
    const next = scope ? this.creativeMemory().filter((item) => item.scope !== scope) : [];
    if (next.length) localStorage.setItem(CREATIVE_MEMORY_KEY, JSON.stringify(next));
    else localStorage.removeItem(CREATIVE_MEMORY_KEY);
    this.creativeMemory.set(next);
  }

  async installLocal(profile: AiModelProfile['id']): Promise<LocalAiStatus> {
    if (!isTauri()) throw new Error('A instalação da IA local precisa ser feita no aplicativo desktop.');
    this.installProgress.set({ stage: 'runtime', percent: 0, downloadedBytes: 0, totalBytes: 0, message: 'Preparando a instalação autorizada…' });
    this.localStatus.update((status) => ({ ...status, state: 'installing', message: 'Preparando a instalação autorizada…' }));
    try {
      const status = await invoke<LocalAiStatus>('install_local_ai', { profile });
      this.localStatus.set(status);
      this.persistSettings({ mode: 'local', endpoint: '', model: '' }, '');
      return await this.waitForLocalEngine();
    } catch (error) {
      await this.refreshLocalStatus().catch(() => undefined);
      throw this.toError(error);
    } finally {
      window.setTimeout(() => this.installProgress.set(null), 1800);
    }
  }

  async refreshLocalStatus(): Promise<LocalAiStatus> {
    if (!isTauri()) return this.localStatus();
    const status = await invoke<LocalAiStatus>('local_ai_status');
    this.localStatus.set(status);
    return status;
  }

  async startLocalEngine(restart = false): Promise<LocalAiStatus> {
    if (!isTauri()) throw new Error('A IA local gerenciada só está disponível no aplicativo desktop.');
    this.localStatus.update((status) => ({ ...status, state: 'starting', message: restart ? 'Reiniciando em modo seguro…' : 'Iniciando a IA junto com o NarraHub…' }));
    await invoke(restart ? 'restart_local_ai_engine' : 'start_local_ai_engine');
    return this.waitForLocalEngine();
  }

  async complete(instruction: string, context: string, options: AiPromptOptions = {}): Promise<string> {
    const settings = this.settings();
    this.validateSettings(settings);
    if (settings.mode === 'off') throw new Error('A assistência por IA está desativada.');
    const localEngine = settings.mode === 'local' ? await this.prepareLocalEngine() : null;
    try {
      return await this.requestCompletion(localEngine?.endpoint || settings.endpoint, localEngine?.model || settings.model, instruction, context, settings.mode, options);
    } catch (error) {
      if (settings.mode === 'local' && this.isConnectionFailure(error)) {
        const recovered = await this.startLocalEngine(true).catch(() => null);
        if (recovered?.running) return this.requestCompletion(MANAGED_ENDPOINT, recovered.modelAlias || 'narrahub-local', instruction, context, settings.mode, options);
      }
      throw this.toFriendlyError(error, settings.mode);
    }
  }

  private async requestCompletion(
    endpoint: string,
    model: string,
    instruction: string,
    context: string,
    mode: Exclude<AiMode, 'off'>,
    options: AiPromptOptions,
    retryAfterEcho = false,
  ): Promise<string> {
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), 120_000);
    try {
      const localSampling = mode === 'local' ? {
        top_p: 0.8,
        top_k: 20,
        min_p: 0,
      } : {};
      const response = await this.httpFetch(`${endpoint}/chat/completions`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', ...(this.sessionApiKey ? { Authorization: `Bearer ${this.sessionApiKey}` } : {}) },
        body: JSON.stringify({
          model,
          temperature: 0.7,
          max_tokens: options.maxTokens ?? (mode === 'local' ? 480 : 1200),
          stream: false,
          ...localSampling,
          messages: buildAiMessages(instruction, context, mode, retryAfterEcho),
        }),
        signal: controller.signal,
      });
      const payload = await response.json().catch(() => ({})) as ChatCompletionResponse;
      if (!response.ok) throw new Error(payload.error?.message || `O provedor respondeu com HTTP ${response.status}.`);
      const content = sanitizeAiCompletion(payload.choices?.[0]?.message?.content || '');
      if (!content) throw new Error('O provedor não retornou texto.');
      if (isAiEcho(content, instruction, options)) {
        if (!retryAfterEcho) return this.requestCompletion(endpoint, model, instruction, context, mode, options, true);
        throw new Error('A IA repetiu a solicitação sem executar a tarefa. Tente um trecho menor ou use o perfil recomendado para este computador.');
      }
      return content;
    } finally { window.clearTimeout(timeout); }
  }

  private async prepareLocalEngine(): Promise<LocalEngine> {
    let status = await this.refreshLocalStatus();
    if (!status.installed) throw new Error('A IA local ainda não está instalada. Abra Configurações e autorize a instalação recomendada.');
    if (!status.running) status = await this.startLocalEngine(status.state === 'error');
    return { endpoint: MANAGED_ENDPOINT, model: status.modelAlias || 'narrahub-local' };
  }

  private async waitForLocalEngine(): Promise<LocalAiStatus> {
    for (let attempt = 0; attempt < 120; attempt++) {
      await new Promise((resolve) => window.setTimeout(resolve, 500));
      const status = await this.refreshLocalStatus();
      if (status.running) return status;
      if (status.state === 'error') throw new Error(status.message);
    }
    throw new Error('A IA local demorou mais de 60 segundos para iniciar. Verifique o diagnóstico nas configurações.');
  }

  private loadSettings(): AiSettings {
    try {
      const stored = JSON.parse(localStorage.getItem(SETTINGS_KEY) || '{}') as Partial<AiSettings>;
      if (stored.mode === 'local' || stored.mode === 'custom' || stored.mode === 'off') return { mode: stored.mode, endpoint: stored.mode === 'local' ? '' : (stored.endpoint || ''), model: stored.mode === 'local' ? '' : (stored.model || '') };
    } catch { /* invalid local preference */ }
    return { mode: 'off', endpoint: '', model: '' };
  }

  private loadCreativeMemory(): AiCreativeMemory[] {
    try {
      const stored = JSON.parse(localStorage.getItem(CREATIVE_MEMORY_KEY) || '[]') as AiCreativeMemory[];
      return Array.isArray(stored) ? stored.filter((item) => item?.scope && item?.summary).slice(-80) : [];
    } catch { return []; }
  }

  private persistSettings(settings: AiSettings, apiKey: string): void {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    if (apiKey.trim()) sessionStorage.setItem(SESSION_API_KEY, apiKey.trim()); else sessionStorage.removeItem(SESSION_API_KEY);
    this.settings.set(settings);
  }

  private validateSettings(settings: AiSettings): void {
    if (settings.mode === 'off' || settings.mode === 'local') return;
    if (!settings.model) throw new Error('Informe o identificador do modelo da sua API.');
    let url: URL;
    try { url = new URL(settings.endpoint); } catch { throw new Error('Informe um endpoint HTTP válido.'); }
    if (!['http:', 'https:'].includes(url.protocol)) throw new Error('O endpoint deve usar HTTP ou HTTPS.');
    if (url.protocol !== 'https:' && !['127.0.0.1', 'localhost', '::1'].includes(url.hostname)) throw new Error('Use HTTPS para uma API remota.');
  }

  private isConnectionFailure(error: unknown): boolean {
    const message = String(error).toLocaleLowerCase('pt-BR');
    return error instanceof TypeError || message.includes('fetch') || message.includes('connection') || message.includes('conexão') || message.includes('reset');
  }

  private toFriendlyError(error: unknown, mode: AiMode): Error {
    if (error instanceof DOMException && error.name === 'AbortError') return new Error('A IA excedeu 120 segundos e foi interrompida.');
    if (mode === 'local' && this.isConnectionFailure(error)) return new Error('O motor local encerrou durante a geração. O NarraHub tentou reiniciá-lo em modo seguro; abra Configurações para ver o diagnóstico ou escolha um perfil mais leve.');
    if (error instanceof Error) return error;
    return new Error(String(error));
  }

  private toError(error: unknown): Error { return error instanceof Error ? error : new Error(String(error)); }

  private async httpFetch(input: string, init?: RequestInit): Promise<Response> {
    if (isTauri()) { const { fetch: nativeFetch } = await import('@tauri-apps/plugin-http'); return nativeFetch(input, init); }
    return fetch(input, init);
  }
}
