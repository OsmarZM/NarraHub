import { Injectable, inject, signal } from '@angular/core';
import { RelationCard } from '../../../core/models';
import { ConnectionsGateway } from '../gateways/connections.gateway';

@Injectable({ providedIn: 'root' })
export class ConnectionsStore {
  private readonly gateway = inject(ConnectionsGateway);
  private requestedUniverseId = '';
  private loadRevision = 0;

  readonly relations = signal<RelationCard[]>([]);
  readonly busy = signal(false);
  readonly error = signal('');

  async load(universeId: string): Promise<void> {
    if (!universeId) { this.reset(); return; }
    const revision = ++this.loadRevision;
    this.requestedUniverseId = universeId;
    this.error.set('');
    try {
      const relations = await this.gateway.listRelations(universeId);
      if (revision === this.loadRevision && this.requestedUniverseId === universeId) this.relations.set(relations);
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar as conexões.');
    }
  }

  reset(): void {
    this.loadRevision += 1;
    this.requestedUniverseId = '';
    this.relations.set([]);
  }

  clearError(): void {
    this.error.set('');
  }

  async create(universeId: string, sourceId: string, targetId: string, label: string): Promise<boolean> {
    const trimmed = label.trim();
    if (!universeId || !sourceId || !targetId || !trimmed || sourceId === targetId) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      await this.gateway.createRelation(universeId, sourceId, targetId, trimmed);
      await this.load(universeId);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível criar a conexão.');
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  async delete(id: string): Promise<boolean> {
    const universeId = this.requestedUniverseId;
    try {
      await this.gateway.deleteRelation(id);
      await this.load(universeId);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível excluir a conexão.');
      return false;
    }
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Connections operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
