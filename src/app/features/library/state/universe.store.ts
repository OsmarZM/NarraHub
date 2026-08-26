import { Injectable, inject, signal } from '@angular/core';
import { Universe, UniverseWithStats } from '../../../core/models';
import { CreateUniverseInput, UniverseGateway, UpdateUniverseInput } from '../gateways/universe.gateway';

@Injectable({ providedIn: 'root' })
export class UniverseStore {
  private readonly gateway = inject(UniverseGateway);
  private loadRevision = 0;

  readonly universes = signal<UniverseWithStats[]>([]);
  readonly busy = signal(false);
  readonly error = signal('');

  async load(): Promise<void> {
    const revision = ++this.loadRevision;
    try {
      const universes = await this.gateway.list();
      if (revision === this.loadRevision) this.universes.set(universes);
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar os universos.');
    }
  }

  get(id: string): Promise<Universe | null> {
    return this.gateway.get(id);
  }

  async create(input: CreateUniverseInput): Promise<UniverseWithStats | null> {
    if (this.busy()) return null;
    this.busy.set(true);
    this.error.set('');
    try {
      const created = await this.gateway.create(input);
      await this.load();
      return this.universes().find((universe) => universe.id === created.id) ?? null;
    } catch (error) {
      this.setError(error, 'Não foi possível criar o universo.');
      return null;
    } finally {
      this.busy.set(false);
    }
  }

  async update(id: string, patch: UpdateUniverseInput): Promise<boolean> {
    return this.mutate(() => this.gateway.update(id, patch));
  }

  async delete(id: string): Promise<boolean> {
    return this.mutate(() => this.gateway.delete(id));
  }

  async refreshStats(id: string): Promise<UniverseWithStats | null> {
    if (!id) return null;
    try {
      const stats = await this.gateway.getStats(id);
      let updated: UniverseWithStats | null = null;
      this.universes.update((items) => items.map((item) => {
        if (item.id !== id) return item;
        updated = { ...item, stats };
        return updated;
      }));
      return updated;
    } catch (error) {
      console.error('[NarraHub] Universe stats refresh failed', error);
      return null;
    }
  }

  clearError(): void {
    this.error.set('');
  }

  private async mutate(operation: () => Promise<void>): Promise<boolean> {
    if (this.busy()) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      await operation();
      await this.load();
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível salvar a alteração do universo.');
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Universe operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
