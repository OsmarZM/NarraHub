import { Component, Input, OnChanges, SimpleChanges, ViewEncapsulation, computed, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { AiService } from '../../../core/services/ai.service';
import { AppState } from '../../../core/state/app.state';
import { WorkspaceSyncService } from '../../../application/workspace-sync.service';
import { KnowledgeStore } from '../../knowledge/state/knowledge.store';
import { ShellState } from '../../../shell/state/shell.state';
import { ContentTag, DEFAULT_ATTRIBUTES, Entity, EntityAttribute, EntityType } from '../../../core/models';
import { fileToDataUrl } from '../../../shared/utils/file-to-data-url';
import { EntityCardComponent } from '../components/entity-card/entity-card.component';
import { EntityToolbarComponent } from '../components/entity-toolbar/entity-toolbar.component';
import { EntitySheetComponent } from '../entity-sheet/entity-sheet.component';
import { EntityHubType, EntityStore } from '../state/entity.store';

export interface EntityMetadataRequest {
  id: string;
  title: string;
}

export type EntityMutationKind = 'created' | 'renamed' | 'updated' | 'deleted';

interface EntityAiAttribute {
  key: string;
  value: string;
}

interface EntityAiDraft {
  name: string;
  description: string;
  attributes: EntityAiAttribute[];
}

type EntityModal = 'create' | 'rename' | 'delete' | null;

@Component({
  selector: 'app-entities-page',
  standalone: true,
  imports: [FormsModule, EntityCardComponent, EntityToolbarComponent, EntitySheetComponent],
  templateUrl: './entities-page.component.html',
  styleUrl: './entities-page.component.css',
  encapsulation: ViewEncapsulation.None,
})
export class EntitiesPageComponent implements OnChanges {
  @Input({ required: true }) universeId = '';

  readonly store = inject(EntityStore);
  readonly ai = inject(AiService);
  private readonly appState = inject(AppState);
  private readonly shell = inject(ShellState);
  private readonly knowledgeStore = inject(KnowledgeStore);
  private readonly sync = inject(WorkspaceSyncService);

  /** Lista x ficha aberta é sub-estado desta página, não navegação. */
  readonly view = signal<'entities' | 'entity-sheet'>('entities');
  get universeName(): string { return this.appState.activeUniverse()?.name ?? ''; }
  get universeDescription(): string { return this.appState.activeUniverse()?.description ?? ''; }
  get query(): string { return this.shell.searchQuery(); }
  readonly modal = signal<EntityModal>(null);
  readonly pendingEntity = signal<Entity | null>(null);
  readonly entityAiBusy = signal(false);
  readonly entityAiError = signal('');
  readonly counts = computed<Record<string, number>>(() => ({
    all: this.store.entities().length,
    Personagem: this.store.typeCount('Personagem'),
    Lugar: this.store.typeCount('Lugar'),
    Evento: this.store.typeCount('Evento'),
    Objeto: this.store.typeCount('Objeto'),
    'Organização': this.store.typeCount('Organização'),
  }));

  newEntityName = '';
  newEntityDescription = '';
  newEntityBrief = '';
  newEntityAiAttributes: EntityAiAttribute[] = [];
  newEntityImageData = '';
  newEntityType: EntityType = 'Personagem';
  renameValue = '';

  constructor() {
    // A busca global vive no shell; refletir aqui mantém o filtro do hub em dia
    // sem a página precisar de um @Input vindo do layout.
    effect(() => this.store.setQuery(this.shell.searchQuery()));
  }

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['universeId']) void this.store.load(this.universeId);
  }

  selectType(type: EntityHubType | null): void {
    this.store.setFilter(type);
    this.view.set('entities');
  }

  async openEntity(entity: Entity): Promise<void> {
    if (await this.store.open(this.universeId, entity)) this.view.set('entity-sheet');
    else this.reportStoreError('Não foi possível abrir a ficha.');
  }

  backToList(): void {
    this.store.clearSelection();
    this.view.set('entities');
  }

  openCreate(): void {
    this.store.clearError();
    this.newEntityType = (this.store.filter() || 'Personagem') as EntityType;
    this.newEntityName = '';
    this.newEntityDescription = '';
    this.newEntityBrief = '';
    this.newEntityAiAttributes = [];
    this.newEntityImageData = '';
    this.entityAiError.set('');
    this.modal.set('create');
  }

  openRename(entity: Entity, sourceEvent?: Event): void {
    sourceEvent?.stopPropagation();
    this.store.clearError();
    this.pendingEntity.set(entity);
    this.renameValue = entity.name;
    this.modal.set('rename');
  }

  openDelete(entity: Entity, sourceEvent?: Event): void {
    sourceEvent?.stopPropagation();
    this.store.clearError();
    this.pendingEntity.set(entity);
    this.modal.set('delete');
  }

  closeModal(): void {
    if (this.store.busy()) return;
    this.pendingEntity.set(null);
    this.modal.set(null);
  }

  async createEntity(): Promise<void> {
    const name = this.newEntityName.trim();
    if (!this.universeId || !name) return;
    const created = await this.store.create({
      universeId: this.universeId,
      type: this.newEntityType,
      name,
      description: this.newEntityDescription,
      image: this.newEntityImageData,
      attributes: this.newEntityAiAttributes,
    });
    if (!created) { this.reportStoreError('Não foi possível criar a entidade.'); return; }
    if (this.newEntityAiAttributes.length) {
      this.ai.remember(this.universeId, 'entity', `Criou ${this.newEntityType.toLocaleLowerCase('pt-BR')} ${name} com os campos ${this.newEntityAiAttributes.map((item) => item.key).join(', ')}.`);
    }
    this.modal.set(null);
    void this.sync.onEntityMutated('created');
    await this.openEntity(created);
  }

  async confirmRename(): Promise<void> {
    const entity = this.pendingEntity();
    const name = this.renameValue.trim();
    if (!entity || !name) return;
    if (!await this.store.rename(this.universeId, entity.id, name)) { this.reportStoreError(`Não foi possível renomear ${entity.name}.`); return; }
    this.pendingEntity.set(null);
    this.modal.set(null);
    void this.sync.onEntityMutated('renamed');
    this.shell.showInfo('Entidade renomeada.');
  }

  async confirmDelete(): Promise<void> {
    const entity = this.pendingEntity();
    if (!entity) return;
    if (!await this.store.delete(this.universeId, entity.id)) { this.reportStoreError(`Não foi possível excluir ${entity.name}.`); return; }
    this.pendingEntity.set(null);
    this.modal.set(null);
    this.view.set('entities');
    void this.sync.onEntityMutated('deleted');
    this.shell.showInfo('Entidade excluída do banco local.');
  }

  async saveActive(): Promise<void> {
    if (!await this.store.saveActive()) { this.reportStoreError('Não foi possível salvar a ficha.'); return; }
    void this.sync.onEntityMutated('updated');
    this.shell.showInfo('Ficha salva.');
  }

  requestMetadata(): void {
    const entity = this.store.activeEntity();
    if (!entity) return;
    void this.knowledgeStore.openMetadata('entity', entity.id, entity.name, this.appState.activeUniverseId());
    this.appState.openModal('metadata');
  }

  activeTags(): ContentTag[] {
    const entityId = this.store.activeEntity()?.id;
    return entityId ? this.knowledgeStore.tagsForOwner('entity', entityId) : [];
  }

  tags(entityId: string): ContentTag[] {
    return this.knowledgeStore.tagsForOwner('entity', entityId);
  }

  defaultFieldsForNewEntity(): string[] {
    return DEFAULT_ATTRIBUTES[this.newEntityType] || [];
  }

  async onImageSelected(event: Event, target: 'new' | 'active'): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) { this.shell.showInfo('Escolha um arquivo de imagem.'); return; }
    if (file.size > 8 * 1024 * 1024) { this.shell.showInfo('A imagem deve ter no máximo 8 MB.'); return; }
    const dataUrl = await fileToDataUrl(file);
    if (target === 'new') { this.newEntityImageData = dataUrl; return; }
    if (await this.store.updateImage(dataUrl)) this.shell.showInfo('Imagem principal atualizada.');
    else this.reportStoreError('Não foi possível atualizar a imagem principal.');
  }

  async removeEntityImage(): Promise<void> {
    if (await this.store.updateImage('')) this.shell.showInfo('Imagem removida.');
    else this.reportStoreError('Não foi possível remover a imagem.');
  }

  async removeAttribute(attribute: EntityAttribute): Promise<void> {
    if (!await this.store.removeAttribute(attribute)) this.reportStoreError('Não foi possível remover a propriedade.');
  }

  async addGalleryImages(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const files = [...(input.files ?? [])];
    input.value = '';
    const accepted = files.filter((file) => file.type.startsWith('image/') && file.size <= 8 * 1024 * 1024).slice(0, 12);
    if (accepted.length !== files.length) this.shell.showInfo('Algumas imagens foram ignoradas: use até 12 arquivos de imagem com no máximo 8 MB cada.');
    const images = await Promise.all(accepted.map(async (file) => ({ dataUrl: await fileToDataUrl(file), caption: file.name })));
    if (images.length && !await this.store.addGalleryImages(this.universeId, images)) this.reportStoreError('Não foi possível adicionar as imagens.');
  }

  async deleteGalleryImage(attachmentId: string): Promise<void> {
    if (!await this.store.deleteGalleryImage(attachmentId)) this.reportStoreError('Não foi possível remover a imagem.');
  }

  async suggestNewEntityWithAi(): Promise<void> {
    if (!this.universeId || this.entityAiBusy()) return;
    if (!this.ai.enabled()) { this.shell.showInfo('Ative a IA nas configurações para montar a ficha.'); return; }
    this.entityAiBusy.set(true);
    this.entityAiError.set('');
    try {
      const response = await this.ai.complete(
        `Crie uma proposta de ${this.newEntityType.toLocaleLowerCase('pt-BR')} coerente com o universo. Use o briefing do escritor sem sobrescrever fatos existentes. Retorne somente JSON válido com {"name":"", "description":"", "attributes":[{"key":"", "value":""}]}. Sugira de 4 a 8 campos úteis e específicos para este tipo de ficha.`,
        this.buildUniverseAiContext(`BRIEFING DA NOVA ENTIDADE:\n${this.newEntityBrief.trim() || 'Sem briefing; proponha algo coerente com o universo.'}`),
      );
      const draft = this.parseAiJson<EntityAiDraft>(response);
      this.newEntityName = String(draft.name || '').trim();
      this.newEntityDescription = String(draft.description || '').trim();
      this.newEntityAiAttributes = this.normalizeAiAttributes(draft.attributes);
    } catch (error) {
      this.entityAiError.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.entityAiBusy.set(false);
    }
  }

  async suggestActiveEntityFieldsWithAi(): Promise<void> {
    const entity = this.store.activeEntity();
    if (!entity || this.entityAiBusy()) return;
    if (!this.ai.enabled()) { this.shell.showInfo('Ative a IA nas configurações para sugerir campos.'); return; }
    this.entityAiBusy.set(true);
    this.entityAiError.set('');
    try {
      const response = await this.ai.complete(
        'Analise esta ficha e sugira apenas propriedades que realmente ajudem a desenvolvê-la. Não repita campos existentes. Retorne somente um array JSON: [{"key":"Nome do campo", "value":"valor sugerido ou vazio"}].',
        this.buildUniverseAiContext(this.entityAiContext(entity)),
      );
      const suggestions = this.normalizeAiAttributes(this.parseAiJson<EntityAiAttribute[]>(response));
      const existing = new Set(entity.attributes.map((item) => this.normalizeSearch(item.key)));
      const additions = suggestions.filter((item) => !existing.has(this.normalizeSearch(item.key))).map((item, index) => ({
        id: `temp_ai_${Date.now()}_${index}`,
        entity_id: entity.id,
        key: item.key,
        value: item.value,
        sort_order: entity.attributes.length + index,
      }));
      this.store.activeEntity.set({ ...entity, attributes: [...entity.attributes, ...additions] });
      if (additions.length) this.ai.remember(entity.universe_id, 'entity', `Aceitou campos sugeridos para ${entity.name}: ${additions.map((item) => item.key).join(', ')}.`);
      else this.shell.showInfo('A IA não encontrou novos campos úteis para esta ficha.');
    } catch (error) {
      this.entityAiError.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.entityAiBusy.set(false);
    }
  }

  async summarizeActiveEntityWithAi(): Promise<void> {
    const entity = this.store.activeEntity();
    if (!entity || this.entityAiBusy()) return;
    if (!this.ai.enabled()) { this.shell.showInfo('Ative a IA nas configurações para resumir a ficha.'); return; }
    this.entityAiBusy.set(true);
    this.entityAiError.set('');
    try {
      const summary = await this.ai.complete(
        `Resuma a ficha de ${entity.type.toLocaleLowerCase('pt-BR')} em um parágrafo curto e útil para consulta. Preserve os fatos, destaque identidade, papel e conflito central e não invente informações.`,
        this.buildUniverseAiContext(this.entityAiContext(entity)),
      );
      this.store.patchActive('summary', summary);
      this.ai.remember(entity.universe_id, 'entity', `Gerou um resumo de ficha para ${entity.name}.`);
    } catch (error) {
      this.entityAiError.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.entityAiBusy.set(false);
    }
  }

  private buildUniverseAiContext(focus: string): string {
    const canon = this.store.entities().slice(0, 40).map((entity) => {
      const description = (entity.summary || entity.description).trim().replace(/\s+/gu, ' ').slice(0, 240);
      return `- ${entity.type}: ${entity.name}${description ? ` — ${description}` : ''}`;
    }).join('\n');
    return [
      `UNIVERSO: ${this.universeName}`,
      this.universeDescription ? `PREMISSA: ${this.universeDescription.slice(0, 1_500)}` : '',
      canon ? `CÂNONE CADASTRADO:\n${canon}` : 'CÂNONE CADASTRADO: ainda vazio',
      this.ai.memoryContext(this.universeId),
      focus,
    ].filter(Boolean).join('\n\n');
  }

  private entityAiContext(entity: NonNullable<ReturnType<EntityStore['activeEntity']>>): string {
    const attributes = entity.attributes.filter((item) => item.key.trim()).map((item) => `- ${item.key}: ${item.value || 'não preenchido'}`).join('\n');
    const relations = entity.relations.slice(0, 20).map((relation) => {
      const other = relation.source.id === entity.id ? relation.target.name : relation.source.name;
      return `- ${relation.label}: ${other}`;
    }).join('\n');
    return [
      `FICHA EM EDIÇÃO\nTipo: ${entity.type}\nNome: ${entity.name}\nResumo: ${entity.summary || 'não preenchido'}\nNotas: ${entity.description || 'não preenchidas'}`,
      `PROPRIEDADES:\n${attributes || 'nenhuma'}`,
      relations ? `RELAÇÕES:\n${relations}` : '',
    ].filter(Boolean).join('\n\n');
  }

  private normalizeAiAttributes(value: unknown): EntityAiAttribute[] {
    if (!Array.isArray(value)) return [];
    const seen = new Set<string>();
    return value.flatMap((item) => {
      if (!item || typeof item !== 'object') return [];
      const record = item as Record<string, unknown>;
      const key = String(record['key'] || '').trim().slice(0, 80);
      const normalized = this.normalizeSearch(key);
      if (!key || seen.has(normalized)) return [];
      seen.add(normalized);
      return [{ key, value: String(record['value'] || '').trim().slice(0, 2_000) }];
    }).slice(0, 10);
  }

  private parseAiJson<T>(response: string): T {
    const clean = response.replace(/```(?:json)?/giu, '').replace(/```/gu, '').trim();
    const objectStart = clean.indexOf('{');
    const arrayStart = clean.indexOf('[');
    const starts = [objectStart, arrayStart].filter((index) => index >= 0);
    const start = starts.length ? Math.min(...starts) : -1;
    if (start < 0) throw new Error('A IA não retornou uma ficha estruturada. Tente novamente.');
    const opening = clean[start];
    const end = clean.lastIndexOf(opening === '[' ? ']' : '}');
    if (end <= start) throw new Error('A IA retornou uma ficha incompleta. Tente novamente.');
    try { return JSON.parse(clean.slice(start, end + 1)) as T; }
    catch { throw new Error('A IA retornou campos em formato inválido. Tente novamente.'); }
  }

  private normalizeSearch(value: string): string {
    return value.normalize('NFD').replace(/[\u0300-\u036f]/gu, '').toLocaleLowerCase('pt-BR').trim();
  }

  private reportStoreError(fallback: string): void {
    this.shell.showError(this.store.error() || fallback);
  }
}
