import { Injectable, computed, inject, signal } from '@angular/core';
import {
  ConflictActionType,
  ConflictDetail,
  ConflictFilter,
  ConflictSummary,
  EpochNotice,
  SyncConflictsService,
} from '../../../core/native/sync-conflicts.service';

/**
 * O estado da tela de conflitos (etapa F).
 *
 * Sem gateway, como o `SettingsStore`: o domínio fala só com comandos Tauri nativos, e não há
 * fronteira SQL-vs-Rust a abstrair. O store nunca vê tabela nem envelope — só os DTOs prontos.
 */
@Injectable({ providedIn: 'root' })
export class ConflictsStore {
  private readonly service = inject(SyncConflictsService);
  private loadRevision = 0;

  readonly conflicts = signal<ConflictSummary[]>([]);
  readonly filter = signal<ConflictFilter>({ universeId: '', aggregateType: '', kind: '', status: 'aberto' });
  readonly selected = signal<ConflictDetail | null>(null);
  readonly loading = signal(false);
  readonly busy = signal(false);
  readonly error = signal('');
  readonly info = signal('');
  readonly epochNotice = signal<EpochNotice | null>(null);

  /** Quantos precisam de atenção, independentemente do filtro. */
  readonly openCount = signal(0);

  readonly visible = computed(() => this.conflicts());

  async load(): Promise<void> {
    const revision = ++this.loadRevision;
    this.loading.set(true);
    this.error.set('');
    try {
      const [filtered, open] = await Promise.all([
        this.service.list(this.filter()),
        this.service.list({ status: 'aberto' }),
      ]);
      if (revision !== this.loadRevision) return;
      this.conflicts.set(filtered);
      this.openCount.set(open.length);
    } catch (error) {
      if (revision === this.loadRevision) this.error.set(this.message(error, 'Os conflitos não puderam ser lidos.'));
    } finally {
      if (revision === this.loadRevision) this.loading.set(false);
    }
  }

  /** Só o número, para a tela de sincronização. */
  async refreshOpenCount(): Promise<void> {
    try {
      this.openCount.set((await this.service.list({ status: 'aberto' })).length);
    } catch {
      // A contagem é auxiliar: a tela de conflitos mostra o erro quando for aberta.
    }
  }

  async loadEpochNotice(): Promise<void> {
    try {
      this.epochNotice.set(await this.service.epochNotice());
    } catch {
      this.epochNotice.set(null);
    }
  }

  async setFilter(patch: Partial<ConflictFilter>): Promise<void> {
    this.filter.update((atual) => ({ ...atual, ...patch }));
    this.selected.set(null);
    await this.load();
  }

  async open(conflictKey: string): Promise<void> {
    this.error.set('');
    this.info.set('');
    try {
      this.selected.set(await this.service.inspect(conflictKey));
    } catch (error) {
      this.error.set(this.message(error, 'O conflito não pôde ser aberto.'));
    }
  }

  close(): void {
    this.selected.set(null);
  }

  async resolve(acao: ConflictActionType, tagId = '', nome = ''): Promise<boolean> {
    const atual = this.selected();
    if (!atual || this.busy()) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      await this.service.resolve(atual.resumo.conflictKey, {
        tipo: acao,
        ...(acao === 'renomear' ? { tagId, nome } : {}),
      });
      this.info.set('Decisão registrada. Ela chega aos outros aparelhos na próxima sincronização.');
      this.selected.set(null);
      await this.load();
      return true;
    } catch (error) {
      this.error.set(this.message(error, 'A decisão não pôde ser registrada.'));
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  private message(error: unknown, fallback: string): string {
    if (error && typeof error === 'object' && 'message' in error) {
      const texto = String((error as { message: unknown }).message || '');
      if (texto) return texto;
    }
    return fallback;
  }
}
