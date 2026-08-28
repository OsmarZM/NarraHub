import { Injectable, inject, signal } from '@angular/core';
import { ContentTag, ContentTagAssignment, Entity, ChapterOption, MentionOccurrence, MetadataOwnerType } from '../../../core/models';
import { UniverseStore } from '../../library/state/universe.store';
import { KnowledgeGateway } from '../gateways/knowledge.gateway';

export interface MetadataTarget {
  type: MetadataOwnerType;
  id: string;
  name: string;
}

@Injectable({ providedIn: 'root' })
export class KnowledgeStore {
  private readonly gateway = inject(KnowledgeGateway);
  private readonly universeStore = inject(UniverseStore);
  private requestedUniverseId = '';
  private loadRevision = 0;

  readonly libraryPreviewTags = signal<Record<string, ContentTag[]>>({});
  readonly workspacePreviewTags = signal<Record<string, ContentTag[]>>({});
  readonly mentionOccurrences = signal<MentionOccurrence[]>([]);
  /** Todas as tags do universo — quem monta seletor de tag (ex.: card de planejamento) lê daqui. */
  readonly universeTags = signal<ContentTag[]>([]);
  readonly metadataTarget = signal<MetadataTarget | null>(null);
  readonly metadataTags = signal<ContentTag[]>([]);
  readonly metadataOwnerTags = signal<ContentTag[]>([]);
  readonly error = signal('');

  /**
   * `force` recarrega mesmo com o universo já carregado. Sem essa guarda o
   * pré-carregamento do layout e o ngOnChanges da página disparavam o mesmo
   * SQL duas vezes a cada entrada na seção. Refresh após mutação passa
   * `force: true` — ali repetir é o objetivo.
   */
  async load(universeId: string, force = false): Promise<void> {
    if (!force && this.requestedUniverseId === universeId) return;
    const revision = ++this.loadRevision;
    this.requestedUniverseId = universeId;
    try {
      const [assignments, tags] = await Promise.all([
        this.gateway.listTagAssignments([universeId]),
        this.gateway.listTags(universeId),
      ]);
      if (revision !== this.loadRevision || this.requestedUniverseId !== universeId) return;
      this.workspacePreviewTags.set(this.groupTagAssignments(assignments));
      this.universeTags.set(tags);
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar as tags do universo.');
    }
  }

  reset(): void {
    this.loadRevision += 1;
    this.requestedUniverseId = '';
    this.workspacePreviewTags.set({});
    this.universeTags.set([]);
    this.mentionOccurrences.set([]);
    this.metadataTarget.set(null);
    this.metadataTags.set([]);
    this.metadataOwnerTags.set([]);
  }

  async refreshLibraryPreviewTags(): Promise<void> {
    const universeIds = this.universeStore.universes().map((universe) => universe.id);
    try {
      const assignments = await this.gateway.listTagAssignments(universeIds, ['universe']);
      this.libraryPreviewTags.set(this.groupTagAssignments(assignments));
    } catch (error) {
      this.setError(error, 'Não foi possível carregar as tags da biblioteca.');
    }
  }

  // ── Modal de tags (universo, história, livro, capítulo, entidade, timeline, planejamento) ──

  async openMetadata(type: MetadataOwnerType, id: string, name: string, activeUniverseId: string | null): Promise<void> {
    const universeId = type === 'universe' ? id : activeUniverseId;
    if (!universeId) return;
    this.metadataTarget.set({ type, id, name });
    const [tags, ownerTags] = await Promise.all([
      this.gateway.listTags(universeId),
      this.gateway.listOwnerTags(type, id),
    ]);
    this.metadataTags.set(tags);
    this.metadataOwnerTags.set(ownerTags);
  }

  closeMetadata(): void {
    this.metadataTarget.set(null);
  }

  isTagAssigned(id: string): boolean {
    return this.metadataOwnerTags().some((tag) => tag.id === id);
  }

  async createTag(name: string, color: string, activeUniverseId: string | null): Promise<boolean> {
    const target = this.metadataTarget();
    const universeId = target?.type === 'universe' ? target.id : activeUniverseId;
    const trimmed = name.trim();
    if (!universeId || !target || !trimmed) return false;
    try {
      const tag = await this.gateway.createTag(universeId, trimmed, color);
      await this.gateway.setTag(target.type, target.id, tag.id, true);
      await this.reloadMetadataTarget(activeUniverseId);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível criar a tag. Verifique se esse nome já existe.');
      return false;
    }
  }

  async toggleTag(tag: ContentTag, activeUniverseId: string | null): Promise<boolean> {
    const target = this.metadataTarget();
    if (!target) return false;
    try {
      await this.gateway.setTag(target.type, target.id, tag.id, !this.isTagAssigned(tag.id));
      await this.reloadMetadataTarget(activeUniverseId);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível atualizar a tag.');
      return false;
    }
  }

  async deleteTag(tag: ContentTag, activeUniverseId: string | null): Promise<boolean> {
    try {
      await this.gateway.deleteTag(tag.id);
      await this.reloadMetadataTarget(activeUniverseId);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível excluir a tag.');
      return false;
    }
  }

  private async reloadMetadataTarget(activeUniverseId: string | null): Promise<void> {
    const target = this.metadataTarget();
    const universeId = target?.type === 'universe' ? target.id : activeUniverseId;
    if (!universeId || !target) return;
    const [tags, ownerTags] = await Promise.all([
      this.gateway.listTags(universeId),
      this.gateway.listOwnerTags(target.type, target.id),
    ]);
    this.metadataTags.set(tags);
    this.metadataOwnerTags.set(ownerTags);
    if (this.requestedUniverseId === universeId) await this.load(universeId, true);
    if (target.type === 'universe') await this.refreshLibraryPreviewTags();
  }

  /** Tags aplicadas a um dono qualquer, na forma que os previews já carregam. */
  tagsForOwner(type: MetadataOwnerType, id: string): ContentTag[] {
    return this.workspacePreviewTags()[`${type}:${id}`] ?? [];
  }

  /** Aplica/remove uma tag num dono fora do modal (ex.: card de planejamento). */
  async setTagOnOwner(type: MetadataOwnerType, id: string, tagId: string, assigned: boolean): Promise<boolean> {
    try {
      await this.gateway.setTag(type, id, tagId, assigned);
      if (this.requestedUniverseId) await this.load(this.requestedUniverseId, true);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível atualizar a tag.');
      return false;
    }
  }

  /** Cria uma tag no universo e já aplica ao dono informado. */
  async createTagForOwner(type: MetadataOwnerType, id: string, name: string, color: string): Promise<boolean> {
    const universeId = this.requestedUniverseId;
    const trimmed = name.trim();
    if (!universeId || !trimmed) return false;
    try {
      const tag = await this.gateway.createTag(universeId, trimmed, color);
      await this.gateway.setTag(type, id, tag.id, true);
      await this.load(universeId, true);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível criar a tag. Verifique se esse nome já existe.');
      return false;
    }
  }

  // ── Menções ──

  async refreshMentionOccurrences(universeId: string): Promise<void> {
    try {
      const mentions = await this.gateway.listMentionsByUniverse(universeId);
      if (this.requestedUniverseId === universeId) this.mentionOccurrences.set(mentions);
    } catch (error) {
      this.setError(error, 'Não foi possível carregar as menções.');
    }
  }

  /** Recalcula as menções de um capítulo recém-salvo a partir do conteúdo e
   * das entidades cadastradas, e atualiza o índice do universo ativo. */
  async syncChapterMentions(chapterId: string, content: string, entities: Entity[]): Promise<void> {
    try {
      await this.gateway.syncChapterMentions(chapterId, this.entityIdsInContent(content, entities));
      if (this.requestedUniverseId) await this.refreshMentionOccurrences(this.requestedUniverseId);
    } catch (error) {
      console.warn('[NarraHub] Não foi possível sincronizar as menções do capítulo.', error);
    }
  }

  /** Reindexa as menções de todos os capítulos de um universo (ex.: ao abrir
   * o universo pela primeira vez, para refletir edições feitas fora do app). */
  async rebuildMentionIndex(universeId: string, chapters: ChapterOption[], entities: Entity[]): Promise<void> {
    try {
      for (const chapter of chapters) {
        if (this.requestedUniverseId !== universeId) return;
        await this.gateway.syncChapterMentions(chapter.id, this.entityIdsInContent(chapter.content || '', entities));
      }
      await this.refreshMentionOccurrences(universeId);
    } catch (error) {
      console.warn('[NarraHub] Não foi possível atualizar o índice de menções.', error);
    }
  }

  private entityIdsInContent(content: string, entities: Entity[]): string[] {
    const text = this.contentParagraphs(content).join(' ');
    return entities.filter((entity) => this.textMentionsEntity(text, entity.name)).map((entity) => entity.id);
  }

  private contentParagraphs(content: string): string[] {
    if (!content.trim()) return [];
    const document = new DOMParser().parseFromString(content, 'text/html');
    const blocks = [...document.querySelectorAll('p, blockquote, h1, h2, h3, li')].map((node) => node.textContent?.trim() || '').filter(Boolean);
    const fallback = document.body.textContent?.trim();
    return blocks.length ? blocks : fallback ? fallback.split(/\n+/u).map((value) => value.trim()).filter(Boolean) : [];
  }

  private textMentionsEntity(text: string, name: string): boolean {
    const normalizedText = this.normalizeSearch(text);
    const normalizedName = this.normalizeSearch(name);
    if (normalizedName.length < 2) return false;
    const escaped = normalizedName.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
    return new RegExp(`(?:^|[^a-z0-9])${escaped}(?:$|[^a-z0-9])`, 'u').test(normalizedText);
  }

  private normalizeSearch(value: string): string {
    return value.normalize('NFD').replace(/[̀-ͯ]/gu, '').toLocaleLowerCase('pt-BR').trim();
  }

  private groupTagAssignments(assignments: ContentTagAssignment[]): Record<string, ContentTag[]> {
    const grouped: Record<string, ContentTag[]> = {};
    for (const assignment of assignments) {
      const key = `${assignment.owner_type}:${assignment.owner_id}`;
      (grouped[key] ??= []).push({
        id: assignment.id,
        universe_id: assignment.universe_id,
        name: assignment.name,
        color: assignment.color,
        created_at: assignment.created_at,
      });
    }
    return grouped;
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Knowledge operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
