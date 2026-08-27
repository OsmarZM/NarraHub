import { CommonModule } from '@angular/common';
import { Component, EventEmitter, Input, OnChanges, Output, SimpleChanges, ViewChild, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { CdkDragDrop, DragDropModule, moveItemInArray } from '@angular/cdk/drag-drop';
import { Book, BookOption, ChapterOption, ContentTag, Entity, MentionOccurrence, Story } from '../../core/models';
import { AiService } from '../../core/services/ai.service';
import { fileToDataUrl } from '../../shared/utils/file-to-data-url';
import { ContextualInspectorComponent } from '../../shell/contextual-inspector/contextual-inspector.component';
import { AiWritingRequest, WritingEditorComponent } from '../writing/writing-editor.component';
import { ManuscriptStore } from './state/manuscript.store';

export interface ManuscriptMetadataRequest {
  type: 'story' | 'book' | 'chapter';
  id: string;
  name: string;
}

interface WritingCharacterInsight {
  entity: Entity;
  mentionedInCurrent: boolean;
  firstOccurrence: MentionOccurrence | null;
  dialogueSnippets: string[];
}

type DeleteKind = 'story' | 'book' | 'chapter';
type RenameKind = 'story' | 'book' | 'chapter';

interface PendingDelete { kind: DeleteKind; id: string; name: string; detail: string }
interface PendingRename { kind: RenameKind; id: string; name: string }

@Component({
  selector: 'app-writing-page',
  standalone: true,
  imports: [CommonModule, FormsModule, DragDropModule, WritingEditorComponent, ContextualInspectorComponent],
  templateUrl: './writing-page.component.html',
  styleUrl: './writing-page.component.css',
})
export class WritingPageComponent implements OnChanges {
  readonly Math = Math;

  @Input({ required: true }) universeId = '';
  @Input() universeName = '';
  @Input() universeDescription = '';
  @Input() entities: Entity[] = [];
  @Input() mentionOccurrences: MentionOccurrence[] = [];
  @Input() tagsByOwner: Record<string, ContentTag[]> = {};
  @Input() focusMode = false;

  @Output() readonly focusModeChange = new EventEmitter<boolean>();
  @Output() readonly fullscreenRequested = new EventEmitter<void>();
  @Output() readonly metadataRequested = new EventEmitter<ManuscriptMetadataRequest>();
  @Output() readonly entityOpenRequested = new EventEmitter<Entity>();
  @Output() readonly info = new EventEmitter<string>();
  @Output() readonly failed = new EventEmitter<string>();

  readonly store = inject(ManuscriptStore);
  readonly ai = inject(AiService);

  readonly pendingDelete = signal<PendingDelete | null>(null);
  readonly pendingRename = signal<PendingRename | null>(null);
  readonly showNewStory = signal(false);
  readonly showNewBook = signal(false);
  readonly showNewChapter = signal(false);

  readonly aiBusy = signal(false);
  readonly aiResponse = signal('');
  readonly aiError = signal('');
  readonly aiWritingRequest = signal<AiWritingRequest | null>(null);
  readonly summaryAiBusy = signal(false);

  newStoryName = '';
  newBookName = '';
  newChapterTitle = '';
  renameValue = '';
  aiPrompt = '';

  readonly wordCount = computed(() => this.countWords(this.store.editorContent()));
  readonly characterEntities = computed(() => this.entities
    .filter((entity) => entity.type === 'Personagem')
    .map(({ id, name, image }) => ({ id, name, image })));
  readonly writingCharacters = computed<WritingCharacterInsight[]>(() => {
    const paragraphs = this.contentParagraphs(this.store.editorContent());
    return this.entities.filter((entity) => entity.type === 'Personagem').map((entity) => {
      const matching = paragraphs.filter((paragraph) => this.textMentionsEntity(paragraph, entity.name));
      const dialogueSnippets = matching.filter((paragraph) => this.looksLikeDialogue(paragraph, entity.name)).slice(0, 3);
      const firstOccurrence = this.mentionOccurrences.find((mention) => mention.entity_id === entity.id) ?? null;
      return { entity, mentionedInCurrent: matching.length > 0, firstOccurrence, dialogueSnippets };
    }).filter((insight) => insight.mentionedInCurrent || insight.firstOccurrence);
  });
  readonly writingPlaces = computed(() => {
    const paragraphs = this.contentParagraphs(this.store.editorContent());
    return this.entities.filter((entity) => entity.type === 'Lugar' && paragraphs.some((paragraph) => this.textMentionsEntity(paragraph, entity.name)));
  });

  @ViewChild(WritingEditorComponent) private writingEditor?: WritingEditorComponent;

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['universeId']) void this.store.load(this.universeId);
  }

  chaptersForBook(bookId: string): ChapterOption[] { return this.store.chaptersForBook(bookId); }
  booksForStory(storyId: string): BookOption[] { return this.store.booksForStory(storyId); }

  toggleStory(story: Story, event: Event): void { event.stopPropagation(); this.store.toggleStory(story.id); }
  toggleBook(book: Book, event: Event): void { event.stopPropagation(); this.store.toggleBook(book.id); }

  async moveTreeChapter(chapter: ChapterOption, direction: -1 | 1, event: Event): Promise<void> {
    event.stopPropagation();
    const items = this.chaptersForBook(chapter.book_id);
    const index = items.findIndex((item) => item.id === chapter.id);
    const next = index + direction;
    if (index < 0 || next < 0 || next >= items.length) return;
    [items[index], items[next]] = [items[next], items[index]];
    await this.store.reorderChaptersInBook(chapter.book_id, items.map((item) => item.id));
  }

  async dropTreeChapter(event: CdkDragDrop<ChapterOption[]>, book: BookOption): Promise<void> {
    if (event.previousIndex === event.currentIndex) return;
    const items = [...event.container.data];
    moveItemInArray(items, event.previousIndex, event.currentIndex);
    await this.store.reorderChaptersInBook(book.id, items.map((item) => item.id));
  }

  async onBookCoverSelected(event: Event, book: Book): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/') || file.size > 8 * 1024 * 1024) { this.info.emit('Escolha uma imagem de até 8 MB.'); return; }
    const coverImage = await fileToDataUrl(file);
    if (await this.store.updateBookCover(book.id, coverImage)) this.info.emit('Capa do livro atualizada.');
    else this.reportStoreError('Não foi possível atualizar a capa do livro.');
  }

  onEditorInput(content: string): void { this.store.setEditorContent(content); }
  onTitleInput(title: string): void { this.store.setEditorTitle(title); }
  onChapterSummaryInput(summary: string): void { this.store.setChapterSummary(summary); }
  async saveChapterNow(): Promise<void> { await this.store.saveNow(); }

  chapterField(key: string): string {
    const chapter = this.store.activeChapter();
    if (!chapter) return '';
    return key === 'Origem da cena' ? chapter.scene_origin : key === 'Destino da cena' ? chapter.scene_destination : '';
  }

  async updateChapterContext(key: string, value: string): Promise<void> {
    const chapter = this.store.activeChapter();
    if (!chapter) return;
    const sceneOrigin = key === 'Origem da cena' ? value.trim() : chapter.scene_origin;
    const sceneDestination = key === 'Destino da cena' ? value.trim() : chapter.scene_destination;
    await this.store.updateSceneRoute(sceneOrigin, sceneDestination);
  }

  previewTags(type: 'story' | 'book' | 'chapter', id: string): ContentTag[] {
    return this.tagsByOwner[`${type}:${id}`] ?? [];
  }

  chapterTags(chapterId: string): ContentTag[] {
    return this.tagsByOwner[`chapter:${chapterId}`] ?? [];
  }

  requestMetadata(type: 'story' | 'book' | 'chapter', id: string, name: string, event?: Event): void {
    event?.stopPropagation();
    this.metadataRequested.emit({ type, id, name });
  }

  openEntity(entity: Entity): void { this.entityOpenRequested.emit(entity); }

  // ── Criação (histórias/livros são autocontidos; capítulo também pode ser aberto pelo cabeçalho do App via ViewChild) ──

  openCreateStory(): void { this.newStoryName = ''; this.showNewStory.set(true); }
  openCreateBook(): void { this.newBookName = ''; this.showNewBook.set(true); }
  openCreateChapter(): void { this.newChapterTitle = ''; this.showNewChapter.set(true); }

  closeCreateModals(): void {
    this.showNewStory.set(false);
    this.showNewBook.set(false);
    this.showNewChapter.set(false);
  }

  async createStory(): Promise<void> {
    const name = this.newStoryName.trim();
    if (!this.universeId || !name) return;
    const created = await this.store.createStory(this.universeId, name);
    if (!created) { this.reportStoreError('Não foi possível criar a história.'); return; }
    this.showNewStory.set(false);
  }

  async createBook(): Promise<void> {
    const story = this.store.activeStory();
    const name = this.newBookName.trim();
    if (!story || !name) return;
    const created = await this.store.createBook(story.id, name);
    if (!created) { this.reportStoreError('Não foi possível criar o livro.'); return; }
    this.showNewBook.set(false);
  }

  async createChapter(): Promise<void> {
    const book = this.store.activeBook();
    const title = this.newChapterTitle.trim();
    if (!book || !title) return;
    const created = await this.store.createChapter(book.id, title);
    if (!created) { this.reportStoreError('Não foi possível criar o capítulo.'); return; }
    this.showNewChapter.set(false);
  }

  // ── Renomear / excluir (autocontidos nesta feature) ──

  requestRename(kind: RenameKind, id: string, name: string, event?: Event): void {
    event?.stopPropagation();
    this.pendingRename.set({ kind, id, name });
    this.renameValue = name;
  }

  renameKindLabel(kind: RenameKind): string {
    return ({ story: 'História', book: 'Livro', chapter: 'Capítulo' } as Record<RenameKind, string>)[kind];
  }

  async confirmRename(): Promise<void> {
    const pending = this.pendingRename();
    const name = this.renameValue.trim();
    if (!pending || !name) return;
    const ok = pending.kind === 'story' ? await this.store.renameStory(pending.id, name)
      : pending.kind === 'book' ? await this.store.renameBook(pending.id, name)
      : await this.store.renameChapter(pending.id, name);
    if (!ok) { this.reportStoreError(`Não foi possível renomear ${pending.name}.`); return; }
    this.pendingRename.set(null);
    this.renameValue = '';
    this.info.emit(`${this.renameKindLabel(pending.kind)} renomeado(a).`);
  }

  requestDelete(kind: DeleteKind, id: string, name: string, event?: Event): void {
    event?.stopPropagation();
    const detail: Record<DeleteKind, string> = {
      story: 'Os livros e capítulos desta história também serão excluídos.',
      book: 'Os capítulos deste livro também serão excluídos.',
      chapter: 'O texto, as revisões e as menções deste capítulo serão excluídos.',
    };
    this.pendingDelete.set({ kind, id, name, detail: detail[kind] });
  }

  deleteKindLabel(kind: DeleteKind): string {
    return ({ story: 'História', book: 'Livro', chapter: 'Capítulo' } as Record<DeleteKind, string>)[kind];
  }

  async confirmDelete(): Promise<void> {
    const pending = this.pendingDelete();
    if (!pending) return;
    const ok = pending.kind === 'story' ? await this.store.deleteStory(pending.id)
      : pending.kind === 'book' ? await this.store.deleteBook(pending.id)
      : await this.store.deleteChapter(pending.id);
    if (!ok) { this.reportStoreError(`Não foi possível excluir ${pending.name}.`); return; }
    this.pendingDelete.set(null);
    this.info.emit(`${this.deleteKindLabel(pending.kind)} excluído(a) do banco local.`);
  }

  // ── IA: resumo de capítulo e assistente de escrita ──

  async summarizeChapterWithAi(): Promise<void> {
    const chapter = this.store.activeChapter();
    if (!chapter || this.summaryAiBusy()) return;
    if (!this.ai.enabled()) { this.info.emit('Ative a IA nas preferências para gerar o resumo.'); return; }
    this.summaryAiBusy.set(true);
    try {
      const text = this.contentParagraphs(this.store.editorContent()).join('\n').slice(-16_000);
      const summary = await this.ai.complete(
        'Resuma este capítulo em um parágrafo objetivo. Inclua acontecimentos, mudança emocional e gancho final. Não invente fatos.',
        this.buildUniverseAiContext(`CAPÍTULO: ${this.store.editorTitle()}\n\nTEXTO:\n${text}`),
      );
      this.store.setChapterSummary(summary);
      await this.store.saveNow();
    } catch (error) {
      this.failed.emit(error instanceof Error ? error.message : String(error));
    } finally {
      this.summaryAiBusy.set(false);
    }
  }

  openAiAssistant(request: AiWritingRequest | null = null): void {
    if (!this.ai.enabled()) { this.info.emit('Configure a IA nas preferências antes de usar o assistente.'); return; }
    if (!request?.instruction.trim()) return;
    this.aiWritingRequest.set(request);
    this.aiPrompt = request.instruction;
    this.aiResponse.set('');
    this.aiError.set('');
    void this.runAiAssistant();
  }

  async runAiAssistant(): Promise<void> {
    if (!this.aiPrompt.trim() || this.aiBusy()) return;
    this.aiBusy.set(true);
    this.aiResponse.set('');
    this.aiError.set('');
    try {
      const selected = this.aiWritingRequest()?.selection?.text;
      const chapterText = selected || this.contentParagraphs(this.store.editorContent()).join('\n').slice(-12_000);
      const characters = this.characterEntities().slice(0, 30).map((character) => `- ${character.name}`).join('\n');
      const context = this.buildUniverseAiContext([
        `CAPÍTULO: ${this.store.editorTitle().trim() || 'Sem título'}`,
        chapterText ? `${selected ? 'TEXTO SELECIONADO' : 'TRECHO ATUAL'}:\n${chapterText}` : 'TRECHO ATUAL: vazio',
        characters ? `PERSONAGENS CADASTRADOS:\n${characters}` : 'PERSONAGENS CADASTRADOS: nenhum',
      ].join('\n\n'));
      const request = this.aiWritingRequest();
      this.aiResponse.set(await this.ai.complete(this.aiPrompt, context, {
        sourceText: selected,
        requireTransformation: request?.action === 'correct' || request?.action === 'rewrite' || request?.action === 'expand' || request?.action === 'shorten',
        maxTokens: request?.action === 'expand' || request?.action === 'chapter' ? 560 : 420,
      }));
    } catch (error) {
      console.error('[NarraHub] A IA não conseguiu concluir a solicitação.', error);
      this.aiError.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.aiBusy.set(false);
    }
  }

  applyAiResponse(mode: 'replace' | 'insert'): void {
    const response = this.aiResponse().trim();
    if (!response) return;
    const request = this.aiWritingRequest();
    const selection = request?.selection;
    if (this.universeId) this.ai.remember(this.universeId, 'writing', `Aceitou a ação “${(request?.instruction || this.aiPrompt).slice(0, 140)}”.`);
    queueMicrotask(() => {
      if (mode === 'replace' && selection) this.writingEditor?.replaceRange(selection.from, selection.to, response);
      else if (request?.insertAt !== undefined) this.writingEditor?.insertAtPosition(request.insertAt, response);
      else this.writingEditor?.insertPlainText(response);
    });
    this.clearAiAssistant();
  }

  clearAiAssistant(): void {
    this.aiWritingRequest.set(null);
    this.aiResponse.set('');
    this.aiError.set('');
    this.aiPrompt = '';
  }

  formatNumber(value: number): string { return value.toLocaleString('pt-BR'); }

  private buildUniverseAiContext(focus: string): string {
    const canon = this.entities.slice(0, 40).map((entity) => {
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

  private looksLikeDialogue(paragraph: string, name: string): boolean {
    const normalized = this.normalizeSearch(paragraph);
    const entityName = this.normalizeSearch(name);
    return /^[—–-]|[“”"]|\b(disse|perguntou|respondeu|sussurrou|gritou|falou)\b/u.test(normalized) || normalized.startsWith(`${entityName}:`);
  }

  private normalizeSearch(value: string): string {
    return value.normalize('NFD').replace(/[̀-ͯ]/gu, '').toLocaleLowerCase('pt-BR').trim();
  }

  private countWords(content: string): number {
    const normalized = content.replace(/<[^>]+>/g, ' ').trim();
    return normalized ? normalized.split(/\s+/u).length : 0;
  }

  private reportStoreError(fallback: string): void {
    this.failed.emit(this.store.error() || fallback);
  }
}
