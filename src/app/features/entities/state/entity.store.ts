import { Injectable, computed, inject, signal } from '@angular/core';
import { Attachment, Entity, EntityAttribute, EntityWithDetails } from '../../../core/models';
import { CreateEntityInput, EntityGateway } from '../gateways/entity.gateway';

export type EntityHubType = 'Personagem' | 'Lugar' | 'Evento' | 'Objeto' | 'Organização';

@Injectable({ providedIn: 'root' })
export class EntityStore {
  private readonly gateway = inject(EntityGateway);
  private loadRevision = 0;
  private selectionRevision = 0;
  private requestedUniverseId = '';

  readonly entities = signal<Entity[]>([]);
  readonly activeEntity = signal<EntityWithDetails | null>(null);
  readonly gallery = signal<Attachment[]>([]);
  readonly filter = signal<EntityHubType | null>(null);
  readonly query = signal('');
  readonly loading = signal(false);
  readonly busy = signal(false);
  readonly error = signal('');

  readonly visibleEntities = computed(() => {
    const filter = this.filter();
    const query = this.query().trim().toLocaleLowerCase('pt-BR');
    return this.entities().filter((entity) =>
      (!filter || entity.type === filter)
      && (!query || `${entity.name} ${entity.summary} ${entity.description} ${entity.type}`.toLocaleLowerCase('pt-BR').includes(query)),
    );
  });

  readonly recentEntities = computed(() =>
    [...this.entities()].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 8),
  );

  async load(universeId: string, force = false): Promise<void> {
    if (!universeId) {
      this.reset();
      return;
    }
    if (!force && this.requestedUniverseId === universeId) return;
    const revision = ++this.loadRevision;
    this.requestedUniverseId = universeId;
    this.loading.set(true);
    this.error.set('');
    try {
      const entities = await this.gateway.list(universeId);
      if (revision === this.loadRevision && this.requestedUniverseId === universeId) this.entities.set(entities);
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar as entidades.');
    } finally {
      if (revision === this.loadRevision) this.loading.set(false);
    }
  }

  listSnapshot(universeId: string): Promise<Entity[]> {
    return this.gateway.list(universeId);
  }

  getDetailsSnapshot(entityId: string): Promise<EntityWithDetails | null> {
    return this.gateway.getWithDetails(entityId);
  }

  setFilter(filter: EntityHubType | null): void {
    this.filter.set(filter);
    this.clearSelection();
  }

  setQuery(query: string): void {
    this.query.set(query);
  }

  typeCount(type: EntityHubType | null): number {
    return type ? this.entities().filter((entity) => entity.type === type).length : this.entities().length;
  }

  createLabel(): string {
    return `Novo ${(this.filter() || 'entidade').toLocaleLowerCase('pt-BR')}`;
  }

  async open(universeId: string, entity: Entity): Promise<boolean> {
    if (!universeId || entity.universe_id !== universeId) return false;
    const revision = ++this.selectionRevision;
    this.error.set('');
    try {
      const [details, gallery] = await Promise.all([
        this.gateway.getWithDetails(entity.id),
        this.gateway.listGallery(universeId, entity.id),
      ]);
      if (revision !== this.selectionRevision || this.requestedUniverseId !== universeId || !details) return false;
      this.activeEntity.set(details);
      this.gallery.set(gallery);
      return true;
    } catch (error) {
      if (revision === this.selectionRevision) this.setError(error, 'Não foi possível abrir a ficha.');
      return false;
    }
  }

  async create(input: CreateEntityInput): Promise<Entity | null> {
    if (this.busy() || !input.universeId || !input.name.trim()) return null;
    this.busy.set(true);
    this.error.set('');
    try {
      const created = await this.gateway.create({ ...input, name: input.name.trim(), description: input.description.trim() });
      await this.load(input.universeId, true);
      return this.entities().find((entity) => entity.id === created.id) ?? created;
    } catch (error) {
      this.setError(error, 'Não foi possível criar a entidade.');
      return null;
    } finally {
      this.busy.set(false);
    }
  }

  async rename(universeId: string, entityId: string, name: string): Promise<boolean> {
    const normalized = name.trim();
    if (!normalized) return false;
    const saved = await this.mutate(() => this.gateway.update(entityId, { name: normalized }));
    if (!saved) return false;
    this.entities.update((items) => items.map((item) => item.id === entityId ? { ...item, name: normalized } : item));
    this.activeEntity.update((item) => item?.id === entityId ? { ...item, name: normalized } : item);
    if (this.requestedUniverseId !== universeId) await this.load(universeId, true);
    return true;
  }

  async delete(universeId: string, entityId: string): Promise<boolean> {
    const saved = await this.mutate(() => this.gateway.delete(entityId));
    if (!saved) return false;
    if (this.activeEntity()?.id === entityId) this.clearSelection();
    await this.load(universeId, true);
    return true;
  }

  patchActive(field: 'name' | 'description' | 'summary' | 'canon_status', value: string): void {
    this.activeEntity.update((entity) => entity ? { ...entity, [field]: value } : entity);
  }

  addAttribute(): void {
    const entity = this.activeEntity();
    if (!entity) return;
    const attribute: EntityAttribute = {
      id: `temp_${Date.now()}`,
      entity_id: entity.id,
      key: 'Nova propriedade',
      value: '',
      sort_order: entity.attributes.length,
    };
    this.activeEntity.set({ ...entity, attributes: [...entity.attributes, attribute] });
  }

  async removeAttribute(attribute: EntityAttribute): Promise<boolean> {
    const entity = this.activeEntity();
    if (!entity) return false;
    if (!attribute.id.startsWith('temp_') && !await this.mutate(() => this.gateway.removeAttribute(attribute.id))) return false;
    this.activeEntity.set({ ...entity, attributes: entity.attributes.filter((item) => item.id !== attribute.id) });
    return true;
  }

  async saveActive(): Promise<boolean> {
    const entity = this.activeEntity();
    if (!entity || this.busy()) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      const name = entity.name.trim();
      await this.gateway.update(entity.id, {
        name,
        description: entity.description,
        summary: entity.summary,
        canon_status: entity.canon_status,
      });
      for (const attribute of entity.attributes) {
        const key = attribute.key.trim();
        if (key) await this.gateway.saveAttribute({ ...attribute, key });
      }
      this.activeEntity.update((item) => item ? { ...item, name } : item);
      this.entities.update((items) => items.map((item) => item.id === entity.id ? {
        ...item,
        name,
        description: entity.description,
        summary: entity.summary,
        canon_status: entity.canon_status,
      } : item));
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível salvar a ficha.');
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  async updateImage(dataUrl: string): Promise<boolean> {
    const entity = this.activeEntity();
    if (!entity) return false;
    if (!await this.mutate(() => this.gateway.update(entity.id, { image: dataUrl }))) return false;
    this.activeEntity.set({ ...entity, image: dataUrl });
    this.entities.update((items) => items.map((item) => item.id === entity.id ? { ...item, image: dataUrl } : item));
    return true;
  }

  async addGalleryImages(universeId: string, files: Array<{ dataUrl: string; caption: string }>): Promise<boolean> {
    const entity = this.activeEntity();
    if (!entity || entity.universe_id !== universeId || !files.length || this.busy()) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      for (const file of files) await this.gateway.createGalleryImage(universeId, entity.id, file.dataUrl, file.caption);
      this.gallery.set(await this.gateway.listGallery(universeId, entity.id));
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível adicionar as imagens.');
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  async deleteGalleryImage(attachmentId: string): Promise<boolean> {
    if (!await this.mutate(() => this.gateway.deleteGalleryImage(attachmentId))) return false;
    this.gallery.update((items) => items.filter((item) => item.id !== attachmentId));
    return true;
  }

  async refreshAfterExternalChange(universeId: string): Promise<void> {
    const activeEntityId = this.activeEntity()?.id;
    await this.load(universeId, true);
    if (!activeEntityId) return;
    const entity = this.entities().find((item) => item.id === activeEntityId);
    if (entity) await this.open(universeId, entity);
    else this.clearSelection();
  }

  clearSelection(): void {
    this.selectionRevision += 1;
    this.activeEntity.set(null);
    this.gallery.set([]);
  }

  clearError(): void {
    this.error.set('');
  }

  reset(): void {
    this.loadRevision += 1;
    this.requestedUniverseId = '';
    this.entities.set([]);
    this.filter.set(null);
    this.query.set('');
    this.loading.set(false);
    this.busy.set(false);
    this.error.set('');
    this.clearSelection();
  }

  private async mutate(operation: () => Promise<void>): Promise<boolean> {
    if (this.busy()) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      await operation();
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível salvar a alteração da entidade.');
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Entity operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
