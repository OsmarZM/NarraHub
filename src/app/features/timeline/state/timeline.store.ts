import { Injectable, inject, signal } from '@angular/core';
import { TimelineEvent } from '../../../core/models';
import { TimelineGateway } from '../gateways/timeline.gateway';
import { CreateTimelineEventInput } from '../models/timeline.models';

@Injectable({ providedIn: 'root' })
export class TimelineStore {
  private readonly gateway = inject(TimelineGateway);
  private loadRevision = 0;

  readonly events = signal<TimelineEvent[]>([]);
  readonly busy = signal(false);
  readonly error = signal('');

  async load(universeId: string): Promise<void> {
    const revision = ++this.loadRevision;
    if (!universeId) {
      this.events.set([]);
      return;
    }
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
      await this.load(universeId);
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

