import { Injectable, inject, signal } from '@angular/core';
import {
  PlanningFieldDefinition, PlanningFieldType, PlanningFieldValues, PlanningItem,
} from '../../../core/models';
import { PlanningCardUpdate, PlanningGateway } from '../gateways/planning.gateway';

@Injectable({ providedIn: 'root' })
export class PlanningStore {
  private readonly gateway = inject(PlanningGateway);
  private requestedUniverseId = '';
  private loadRevision = 0;

  readonly items = signal<PlanningItem[]>([]);
  readonly fieldDefinitions = signal<PlanningFieldDefinition[]>([]);
  readonly error = signal('');

  /**
   * `force` recarrega mesmo com o universo já carregado. Sem essa guarda o
   * pré-carregamento do layout e o ngOnChanges da página disparavam o mesmo
   * SQL duas vezes a cada entrada na seção. Refresh após mutação passa
   * `force: true` — ali repetir é o objetivo.
   */
  async load(universeId: string, force = false): Promise<void> {
    if (!universeId) { this.reset(); return; }
    if (!force && this.requestedUniverseId === universeId) return;
    const revision = ++this.loadRevision;
    this.requestedUniverseId = universeId;
    this.error.set('');
    try {
      const [items, definitions] = await Promise.all([
        this.gateway.list(universeId),
        this.gateway.listFieldDefinitions(universeId),
      ]);
      if (revision !== this.loadRevision || this.requestedUniverseId !== universeId) return;
      this.items.set(items);
      this.fieldDefinitions.set(definitions);
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar o planejamento.');
    }
  }

  reset(): void {
    this.loadRevision += 1;
    this.requestedUniverseId = '';
    this.items.set([]);
    this.fieldDefinitions.set([]);
  }

  clearError(): void { this.error.set(''); }

  /** Reordenar é otimista: o quadro já mostrou o card na coluna nova ao soltar. */
  setItems(items: PlanningItem[]): void { this.items.set(items); }

  async refreshItems(): Promise<void> {
    if (!this.requestedUniverseId) return;
    try {
      this.items.set(await this.gateway.list(this.requestedUniverseId));
    } catch (error) {
      this.setError(error, 'Não foi possível recarregar o planejamento.');
    }
  }

  async create(title: string, description: string, chapterId: string | null, image: string): Promise<string | null> {
    const universeId = this.requestedUniverseId;
    if (!universeId) return null;
    try {
      return await this.gateway.create(universeId, title, description, chapterId, image);
    } catch (error) {
      this.setError(error, 'Não foi possível criar o card.');
      return null;
    }
  }

  async saveCard(id: string, update: PlanningCardUpdate): Promise<boolean> {
    return this.run(() => this.gateway.saveCard(id, this.requestedUniverseId, update), 'Não foi possível salvar o card.');
  }

  async saveOrder(items: PlanningItem[]): Promise<boolean> {
    return this.run(() => this.gateway.saveOrder(this.requestedUniverseId, items), 'Não foi possível salvar a ordem.');
  }

  async delete(id: string): Promise<boolean> {
    return this.run(() => this.gateway.delete(id, this.requestedUniverseId), 'Não foi possível excluir o card.');
  }

  listFieldLinks(cardId: string): Promise<PlanningFieldValues> {
    return this.gateway.listFieldLinks(cardId);
  }

  async refreshFieldDefinitions(): Promise<void> {
    if (!this.requestedUniverseId) return;
    try {
      this.fieldDefinitions.set(await this.gateway.listFieldDefinitions(this.requestedUniverseId));
    } catch (error) {
      this.setError(error, 'Não foi possível carregar os campos.');
    }
  }

  async createFieldDefinition(name: string, fieldType: PlanningFieldType, options: string[]): Promise<PlanningFieldDefinition | null> {
    const universeId = this.requestedUniverseId;
    if (!universeId) return null;
    try {
      const definition = await this.gateway.createFieldDefinition(universeId, name, fieldType, options);
      await this.refreshFieldDefinitions();
      return definition;
    } catch (error) {
      this.setError(error, 'Não foi possível criar o campo.');
      return null;
    }
  }

  async renameFieldDefinition(id: string, name: string): Promise<boolean> {
    return this.run(() => this.gateway.renameFieldDefinition(id, this.requestedUniverseId, name), 'Não foi possível renomear o campo.');
  }

  async deleteFieldDefinition(id: string): Promise<boolean> {
    return this.run(() => this.gateway.deleteFieldDefinition(id, this.requestedUniverseId), 'Não foi possível excluir o campo.');
  }

  private async run(operation: () => Promise<void>, fallback: string): Promise<boolean> {
    try {
      await operation();
      return true;
    } catch (error) {
      this.setError(error, fallback);
      return false;
    }
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Planning operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
