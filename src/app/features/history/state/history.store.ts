import { Injectable, inject, signal } from '@angular/core';
import { HistoryEntry } from '../../../core/models';
import { HistoryGateway } from '../gateways/history.gateway';

@Injectable({ providedIn: 'root' })
export class HistoryStore {
  private readonly gateway = inject(HistoryGateway);
  private loadRevision = 0;

  readonly entries = signal<HistoryEntry[]>([]);
  readonly loading = signal(false);
  readonly error = signal('');

  async load(universeId: string): Promise<void> {
    const revision = ++this.loadRevision;
    if (!universeId) {
      this.entries.set([]);
      return;
    }
    this.loading.set(true);
    this.error.set('');
    try {
      const entries = await this.gateway.listRecent(universeId);
      if (revision === this.loadRevision) this.entries.set(entries);
    } catch (error) {
      if (revision === this.loadRevision) {
        console.error('[NarraHub] History load failed', error);
        this.error.set('Não foi possível carregar o histórico local.');
      }
    } finally {
      if (revision === this.loadRevision) this.loading.set(false);
    }
  }

  reset(): void {
    this.loadRevision += 1;
    this.entries.set([]);
    this.loading.set(false);
    this.error.set('');
  }
}

