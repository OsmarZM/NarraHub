import { Injectable, inject, signal } from '@angular/core';
import { TimelineEvent } from '../../../core/models';
import { TimelineGateway } from '../gateways/timeline.gateway';
import { CreateTimelineEventInput } from '../models/timeline.models';

@Injectable({ providedIn: 'root' })
export class TimelineStore {
  private readonly gateway = inject(TimelineGateway);
  private loadRevision = 0;
  private loadedUniverseId: string | null = null;

  readonly events = signal<TimelineEvent[]>([]);
  readonly busy = signal(false);
  readonly error = signal('');

  /**
   * `force` recarrega mesmo com o universo já carregado. Sem essa guarda o
   * pré-carregamento do layout e o ngOnChanges da página disparavam o mesmo
   * SQL duas vezes a cada entrada na seção. Refresh após mutação passa
   * `force: true` — ali repetir é o objetivo.
   */
  async load(universeId: string, force = false): Promise<void> {
    if (!universeId) {
      this.loadedUniverseId = null;
      this.loadRevision++;
      this.events.set([]);
      return;
    }
    // A guarda vem ANTES de mexer em loadRevision: sair por aqui depois de
    // incrementar invalidaria a carga que já está em voo e a tela ficaria vazia.
    if (!force && this.loadedUniverseId === universeId) return;
    this.loadedUniverseId = universeId;
    const revision = ++this.loadRevision;
    try {
      const events = await this.gateway.list(universeId);
      if (revision === this.loadRevision) this.events.set(events);
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar a linha do tempo.');
    }
  }

  async create(universeId: string, input: CreateTimelineEventInput): Promise<boolean> {
    return this.mutate(universeId, () => this.gateway.create(universeId, input));
  }

  async rename(universeId: string, eventId: string, title: string): Promise<boolean> {
    return this.mutate(universeId, () => this.gateway.rename(eventId, title));
  }

  async delete(universeId: string, eventId: string): Promise<boolean> {
    return this.mutate(universeId, () => this.gateway.delete(eventId));
  }

  clearError(): void {
    this.error.set('');
  }

  reset(): void {
    this.loadRevision += 1;
    this.events.set([]);
    this.error.set('');
    this.busy.set(false);
  }

  private async mutate(universeId: string, operation: () => Promise<void>): Promise<boolean> {
    if (!universeId || this.busy()) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      await operation();
      await this.load(universeId, true);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível salvar a alteração na linha do tempo.');
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Timeline operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}

