import { CommonModule } from '@angular/common';
import { CdkDragDrop, DragDropModule } from '@angular/cdk/drag-drop';
import { Component, EventEmitter, Input, OnChanges, Output, SimpleChanges, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';
import { ManuscriptStore } from '../manuscript/state/manuscript.store';
import { EntityStore } from '../entities/state/entity.store';
import {
  ChapterOption,
  ContentTag,
  ContentTagAssignment,
  Entity,
  PlanningFieldDefinition,
  PlanningFieldType,
  PlanningFieldValue,
  PlanningFieldValues,
  PlanningItem,
  PlanningStatus,
  Story,
} from '../../core/models';
import { KnowledgeStore } from '../knowledge/state/knowledge.store';
import { PlanningStore } from './state/planning.store';
import {
  PLANNING_STATUSES,
  parsePlanningFieldValues,
  reorderPlanningItems,
} from './planning-board.utils';

interface FieldTypeOption {
  value: PlanningFieldType;
  label: string;
  hint: string;
}

@Component({
  selector: 'app-planning-board',
  standalone: true,
  imports: [CommonModule, FormsModule, DragDropModule],
  templateUrl: './planning-board.component.html',
  styleUrl: './planning-board.component.css',
})
export class PlanningBoardComponent implements OnChanges {
  readonly Math = Math;
  @Input({ required: true }) universeId = '';
  /**
   * NÃO declarar como @Input: withComponentInputBinding() zera todo input que a
   * rota não alimenta, ignorando o inicializador da classe. Página roteada lê
   * direto dos stores dos domínios que precisa referenciar.
   */
  get chapters(): ChapterOption[] { return this.manuscriptStore.universeChapters(); }
  get stories(): Story[] { return this.manuscriptStore.stories(); }
  get entities(): Entity[] { return this.entityStore.entities(); }

  readonly statuses = PLANNING_STATUSES;
  readonly boardItems = signal<PlanningItem[]>([]);
  readonly definitions = signal<PlanningFieldDefinition[]>([]);
  readonly tags = signal<ContentTag[]>([]);
  readonly tagsByCard = signal<Record<string, ContentTag[]>>({});
  readonly cardTagIds = signal<Set<string>>(new Set());
  readonly modal = signal<'create' | 'card' | null>(null);
  readonly busy = signal(false);
  readonly error = signal('');
  readonly dragging = signal(false);
  readonly deleteConfirmation = signal(false);
  readonly pendingFieldDeleteId = signal<string | null>(null);
  readonly fieldBuilderOpen = signal(false);
  readonly searchQuery = signal('');
  readonly filterStatus = signal<PlanningStatus | null>(null);

  editingCard: PlanningItem | null = null;
  cardFieldValues: PlanningFieldValues = {};
  newTitle = '';
  newDescription = '';
  newChapterId = '';
  newImage = '';
  newStatus: PlanningStatus = 'IDEIAS';
  newFieldName = '';
  newFieldType: PlanningFieldType = 'text';
  newFieldOptions = '';
  newTagName = '';
  newTagColor = '#7d3650';
  editingFieldId: string | null = null;
  editingFieldName = '';

  readonly fieldTypes: FieldTypeOption[] = [
    { value: 'text', label: 'Texto curto', hint: 'Nome, referência ou observação breve' },
    { value: 'long_text', label: 'Texto longo', hint: 'Briefing, descrição ou notas extensas' },
    { value: 'number', label: 'Número', hint: 'Estimativa, peso ou prioridade numérica' },
    { value: 'checkbox', label: 'Caixa de seleção', hint: 'Marcado ou desmarcado' },
    { value: 'yes_no', label: 'Sim ou não', hint: 'Resposta explícita e opcional' },
    { value: 'select', label: 'Lista', hint: 'Uma opção criada pelo usuário' },
    { value: 'multi_select', label: 'Lista múltipla', hint: 'Uma ou mais opções personalizadas' },
    { value: 'tags', label: 'Tags', hint: 'Tags existentes neste universo' },
    { value: 'story', label: 'Histórias relacionadas', hint: 'Uma ou mais histórias do universo' },
    { value: 'character', label: 'Personagens relacionados', hint: 'Uma ou mais fichas de personagem' },
  ];

  private readonly planningStore = inject(PlanningStore);
  private readonly knowledgeStore = inject(KnowledgeStore);
  private readonly manuscriptStore = inject(ManuscriptStore);
  private readonly entityStore = inject(EntityStore);
  private readonly router = inject(Router);

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['universeId'] && this.universeId) void this.reload();
  }

  openCreate(defaultStatus: PlanningStatus = 'IDEIAS'): void {
    this.error.set('');
    this.newTitle = '';
    this.newDescription = '';
    this.newChapterId = '';
    this.newImage = '';
    this.newStatus = defaultStatus || 'IDEIAS';
    this.modal.set('create');
  }

  statusCount(status: PlanningStatus): number {
    return this.boardItems().filter((item) => item.status === status).length;
  }

  toggleStatusFilter(status: PlanningStatus): void {
    this.filterStatus.update((current) => (current === status ? null : status));
  }

  getChapter(chapterId: string | null | undefined): ChapterOption | undefined {
    if (!chapterId) return undefined;
    return this.chapters.find((item) => item.id === chapterId);
  }

  async createCard(): Promise<void> {
    const chapter = this.chapters.find((item) => item.id === this.newChapterId);
    const title = this.newTitle.trim() || chapter?.title || '';
    if (!this.universeId || !title || this.busy()) return;
    this.busy.set(true);
    this.error.set('');
    try {
      const id = await this.planningStore.create(
        title,
        this.newDescription,
        chapter?.id ?? null,
        this.newImage,
      );
      if (!id) { this.showError(this.planningStore.error(), 'Não foi possível criar o card.'); return; }
      if (this.newStatus && this.newStatus !== 'IDEIAS') {
        await this.planningStore.saveCard(id, {
          title,
          description: this.newDescription,
          image: this.newImage,
          status: this.newStatus,
          chapterId: chapter?.id ?? null,
          fieldValues: {},
        });
      }
      await this.refresh();
      const created = this.boardItems().find((item) => item.id === id);
      if (created) await this.openCard(created);
      else this.modal.set(null);
    } catch (error) {
      this.showError(error);
    } finally {
      this.busy.set(false);
    }
  }

  async openCard(item: PlanningItem): Promise<void> {
    if (this.dragging()) return;
    this.error.set('');
    this.deleteConfirmation.set(false);
    this.pendingFieldDeleteId.set(null);
    this.fieldBuilderOpen.set(false);
    this.editingCard = { ...item };
    const scalarValues = parsePlanningFieldValues(item.custom_field_values);
    this.cardFieldValues = scalarValues;
    this.modal.set('card');
    try {
      const relationValues = await this.planningStore.listFieldLinks(item.id);
      this.cardFieldValues = { ...scalarValues, ...relationValues };
      this.syncTagsFromKnowledge(item.id);
    } catch (error) {
      this.showError(error);
    }
  }

  closeModal(): void {
    if (this.busy()) return;
    this.modal.set(null);
    this.editingCard = null;
    this.deleteConfirmation.set(false);
    this.pendingFieldDeleteId.set(null);
  }

  itemsByStatus(status: PlanningStatus): PlanningItem[] {
    const activeFilter = this.filterStatus();
    if (activeFilter && activeFilter !== status) return [];
    const query = this.searchQuery().trim().toLowerCase();
    const items = this.boardItems().filter((item) => item.status === status);
    if (!query) return items;
    return items.filter((item) => {
      const matchTitle = (item.title || '').toLowerCase().includes(query);
      const matchDesc = (item.description || '').toLowerCase().includes(query);
      const matchStory = (item.story_name || '').toLowerCase().includes(query);
      const matchBook = (item.book_name || '').toLowerCase().includes(query);
      const matchChapter = (item.chapter_title || '').toLowerCase().includes(query);
      const tags = this.cardTags(item.id);
      const matchTag = tags.some((t) => (t.name || '').toLowerCase().includes(query));
      return matchTitle || matchDesc || matchStory || matchBook || matchChapter || matchTag;
    });
  }

  async drop(event: CdkDragDrop<PlanningItem[]>, targetStatus: PlanningStatus): Promise<void> {
    const item = event.item.data as PlanningItem | undefined;
    if (!item || this.busy()) return;
    const previous = this.boardItems();
    const reordered = reorderPlanningItems(previous, item.id, targetStatus, event.currentIndex);
    this.boardItems.set(reordered);
    this.planningStore.setItems(reordered);
    this.busy.set(true);
    try {
      await this.planningStore.saveOrder(reordered);
      await this.refresh(false);
    } catch (error) {
      this.boardItems.set(previous);
      this.planningStore.setItems(previous);
      this.showError(error);
    } finally {
      this.busy.set(false);
    }
  }

  async moveOne(item: PlanningItem, direction: -1 | 1, event: MouseEvent): Promise<void> {
    event.stopPropagation();
    const nextIndex = this.statuses.indexOf(item.status) + direction;
    if (nextIndex < 0 || nextIndex >= this.statuses.length) return;
    const targetStatus = this.statuses[nextIndex];
    const reordered = reorderPlanningItems(this.boardItems(), item.id, targetStatus, this.itemsByStatus(targetStatus).length);
    this.boardItems.set(reordered);
    this.planningStore.setItems(reordered);
    try {
      await this.planningStore.saveOrder(reordered);
      await this.refresh(false);
    } catch (error) {
      await this.refresh(false);
      this.showError(error);
    }
  }

  dragStarted(): void {
    this.dragging.set(true);
  }

  dragEnded(): void {
    window.setTimeout(() => this.dragging.set(false), 0);
  }

  async saveCard(): Promise<void> {
    if (!this.editingCard || !this.editingCard.title.trim() || this.busy()) return;
    this.busy.set(true);
    this.error.set('');
    try {
      await this.planningStore.saveCard(this.editingCard.id, {
        title: this.editingCard.title,
        description: this.editingCard.description,
        image: this.editingCard.image,
        status: this.editingCard.status,
        chapterId: this.editingCard.chapter_id || null,
        fieldValues: this.cardFieldValues,
      });
      await this.refresh();
      this.closeModalAfterBusy();
    } catch (error) {
      this.showError(error);
    } finally {
      this.busy.set(false);
    }
  }

  async deleteCard(): Promise<void> {
    if (!this.editingCard || this.busy()) return;
    if (!this.deleteConfirmation()) {
      this.deleteConfirmation.set(true);
      return;
    }
    this.busy.set(true);
    try {
      await this.planningStore.delete(this.editingCard.id);
      await this.refresh();
      this.closeModalAfterBusy();
    } catch (error) {
      this.showError(error);
    } finally {
      this.busy.set(false);
    }
  }

  openLinkedChapter(item: PlanningItem, event?: MouseEvent): void {
    event?.stopPropagation();
    if (!item.chapter_id) return;
    // Abrir o capítulo do card é navegação: sai do quadro e vai para Escrita.
    const option = this.manuscriptStore.universeChapters().find((chapter) => chapter.id === item.chapter_id);
    if (!option) return;
    void this.manuscriptStore.openChapterOption(option).then((opened) => {
      if (opened) void this.router.navigate(['/workspace', this.universeId, 'writing']);
    });
  }

  fieldText(fieldId: string): string {
    const value = this.cardFieldValues[fieldId];
    return typeof value === 'string' ? value : '';
  }

  fieldChecked(fieldId: string): boolean {
    return this.cardFieldValues[fieldId] === true;
  }

  fieldArray(fieldId: string): string[] {
    const value = this.cardFieldValues[fieldId];
    return Array.isArray(value) ? value : [];
  }

  setFieldValue(fieldId: string, value: PlanningFieldValue): void {
    this.cardFieldValues = { ...this.cardFieldValues, [fieldId]: value };
  }

  toggleArrayValue(fieldId: string, value: string, checked: boolean): void {
    const current = new Set(this.fieldArray(fieldId));
    if (checked) current.add(value); else current.delete(value);
    this.setFieldValue(fieldId, [...current]);
  }

  fieldOptions(field: PlanningFieldDefinition): string[] {
    try {
      const options = JSON.parse(field.options_json);
      return Array.isArray(options) ? options.filter((item): item is string => typeof item === 'string') : [];
    } catch {
      return [];
    }
  }

  async createField(): Promise<void> {
    if (!this.newFieldName.trim() || this.busy()) return;
    const options = this.parseNewFieldOptions();
    if (this.requiresOptions(this.newFieldType) && !options.length) {
      this.error.set('Adicione ao menos uma opção, uma por linha, para este tipo de campo.');
      return;
    }
    this.busy.set(true);
    this.error.set('');
    try {
      const definition = await this.planningStore.createFieldDefinition(
        this.newFieldName,
        this.newFieldType,
        options,
      );
      if (!definition) { this.showError(this.planningStore.error(), 'Não foi possível criar o campo.'); return; }
      this.definitions.set(this.planningStore.fieldDefinitions());
      this.newFieldName = '';
      this.newFieldOptions = '';
      this.newFieldType = 'text';
      this.fieldBuilderOpen.set(false);
    } catch (error) {
      this.showError(error, 'Já existe um campo com esse nome neste universo.');
    } finally {
      this.busy.set(false);
    }
  }

  startRenameField(field: PlanningFieldDefinition): void {
    this.editingFieldId = field.id;
    this.editingFieldName = field.name;
  }

  async saveFieldName(field: PlanningFieldDefinition): Promise<void> {
    if (!this.editingFieldName.trim() || this.busy()) return;
    this.busy.set(true);
    try {
      await this.planningStore.renameFieldDefinition(field.id, this.editingFieldName);
      this.definitions.update((items) => items.map((item) => item.id === field.id ? { ...item, name: this.editingFieldName.trim() } : item));
      this.editingFieldId = null;
    } catch (error) {
      this.showError(error, 'Não foi possível renomear o campo. Verifique se o nome já está em uso.');
    } finally {
      this.busy.set(false);
    }
  }

  async deleteField(field: PlanningFieldDefinition): Promise<void> {
    if (this.pendingFieldDeleteId() !== field.id) {
      this.pendingFieldDeleteId.set(field.id);
      return;
    }
    this.busy.set(true);
    try {
      await this.planningStore.deleteFieldDefinition(field.id);
      this.definitions.update((items) => items.filter((item) => item.id !== field.id));
      const { [field.id]: _removed, ...remaining } = this.cardFieldValues;
      this.cardFieldValues = remaining;
      this.pendingFieldDeleteId.set(null);
    } catch (error) {
      this.showError(error);
    } finally {
      this.busy.set(false);
    }
  }

  async toggleCardTag(tag: ContentTag, assigned: boolean): Promise<void> {
    if (!this.editingCard) return;
    try {
      await this.knowledgeStore.setTagOnOwner('planning', this.editingCard.id, tag.id, assigned);
      const next = new Set(this.cardTagIds());
      if (assigned) next.add(tag.id); else next.delete(tag.id);
      this.cardTagIds.set(next);
      this.syncTagsFromKnowledge();
    } catch (error) {
      this.showError(error);
    }
  }

  async createAndAssignTag(): Promise<void> {
    if (!this.editingCard || !this.newTagName.trim()) return;
    try {
      if (!await this.knowledgeStore.createTagForOwner('planning', this.editingCard.id, this.newTagName, this.newTagColor)) {
        this.showError(this.knowledgeStore.error(), 'Não foi possível criar a tag. Verifique se o nome já está em uso.');
        return;
      }
      this.newTagName = '';
      this.syncTagsFromKnowledge();
    } catch (error) {
      this.showError(error, 'Não foi possível criar a tag. Verifique se o nome já está em uso.');
    }
  }

  cardTags(itemId: string): ContentTag[] {
    return this.tagsByCard()[itemId] ?? [];
  }

  statusLabel(status: PlanningStatus): string {
    return status === 'REVISAO' ? 'Revisão' : status.charAt(0) + status.slice(1).toLocaleLowerCase('pt-BR');
  }

  fieldTypeLabel(type: PlanningFieldType): string {
    return this.fieldTypes.find((item) => item.value === type)?.label ?? type;
  }

  requiresOptions(type = this.newFieldType): boolean {
    return type === 'select' || type === 'multi_select';
  }

  characters(): Entity[] {
    return this.entities.filter((entity) => entity.type === 'Personagem');
  }

  // O quadro tem mais etapas do que cabem na largura da janela. Um mouse
  // comum só rola na vertical, então convertemos essa rolagem em
  // deslocamento horizontal (trackpad e toque continuam rolando na lateral
  // normalmente, sem interferência).
  onBoardWheel(event: WheelEvent): void {
    if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    const board = event.currentTarget as HTMLElement;
    if (board.scrollWidth <= board.clientWidth) return;
    event.preventDefault();
    board.scrollLeft += event.deltaY;
  }

  onImageSelected(event: Event, target: 'new' | 'card'): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) {
      this.error.set('Escolha um arquivo de imagem válido.');
      return;
    }
    if (file.size > 4 * 1024 * 1024) {
      this.error.set('A imagem deve ter no máximo 4 MB para não tornar o banco local pesado.');
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      const dataUrl = String(reader.result || '');
      if (target === 'new') this.newImage = dataUrl;
      else if (this.editingCard) this.editingCard = { ...this.editingCard, image: dataUrl };
    };
    reader.onerror = () => this.error.set('Não foi possível ler a imagem escolhida.');
    reader.readAsDataURL(file);
  }

  /** Recarrega tudo que o quadro mostra: cards, campos e tags. */
  private async reload(): Promise<void> {
    await this.planningStore.load(this.universeId, true);
    this.syncFromStores();
  }

  private async refresh(includeMetadata = true): Promise<void> {
    await this.planningStore.refreshItems();
    this.boardItems.set(this.normalizeItems(this.planningStore.items()));
    if (includeMetadata) {
      await this.planningStore.refreshFieldDefinitions();
      this.definitions.set(this.planningStore.fieldDefinitions());
      this.syncTagsFromKnowledge();
    }
  }

  private syncFromStores(): void {
    this.boardItems.set(this.normalizeItems(this.planningStore.items()));
    this.definitions.set(this.planningStore.fieldDefinitions());
    this.syncTagsFromKnowledge();
  }

  /**
   * As tags vêm do KnowledgeStore, que já mantém as atribuições do universo
   * agrupadas por dono. O quadro só recorta a fatia 'planning' — não fala com
   * o serviço de metadados.
   */
  private syncTagsFromKnowledge(activeCardId = this.editingCard?.id): void {
    this.tags.set(this.knowledgeStore.universeTags());
    const byCard: Record<string, ContentTag[]> = {};
    for (const item of this.boardItems()) {
      const applied = this.knowledgeStore.tagsForOwner('planning', item.id);
      if (applied.length) byCard[item.id] = applied;
    }
    this.tagsByCard.set(byCard);
    this.cardTagIds.set(new Set(activeCardId ? (byCard[activeCardId] ?? []).map((tag) => tag.id) : []));
  }

  private normalizeItems(items: PlanningItem[]): PlanningItem[] {
    return items.map((item) => ({
      ...item,
      image: item.image || '',
      custom_field_values: item.custom_field_values || '{}',
      sort_order: Number(item.sort_order) || 0,
    }));
  }

  private parseNewFieldOptions(): string[] {
    return [...new Set(this.newFieldOptions.split(/\r?\n|,/u).map((item) => item.trim()).filter(Boolean))];
  }

  private closeModalAfterBusy(): void {
    this.modal.set(null);
    this.editingCard = null;
    this.deleteConfirmation.set(false);
  }

  private showError(error: unknown, fallback = 'Não foi possível concluir a operação no planejamento.'): void {
    console.error('[NarraHub] Planning operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
