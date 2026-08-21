import { CommonModule } from '@angular/common';
import { Component, HostListener, OnDestroy, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { CdkDragDrop, DragDropModule, moveItemInArray } from '@angular/cdk/drag-drop';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Attachment, Book, BookOption, Chapter, ChapterOption, Entity, EntityType, EntityWithDetails, HistoryEntry,
  MentionOccurrence, PlanningItem, PlanningStatus, RelationCard, Story, SyncServerStatus, TimelineEvent, UniverseWithStats,
} from './core/models';
import { BookService } from './core/services/book.service';
import { AttachmentService } from './core/services/attachment.service';
import { ChapterService } from './core/services/chapter.service';
import { DatabaseService } from './core/services/database.service';
import { EntityService } from './core/services/entity.service';
import { MentionService } from './core/services/mention.service';
import { OnlineShareService } from './core/services/online-share.service';
import { StoryService } from './core/services/story.service';
import { SyncService } from './core/services/sync.service';
import { ThemePreference, ThemeService } from './core/services/theme.service';
import { AppUpdateInfo, UpdateService } from './core/services/update.service';
import { UniverseService } from './core/services/universe.service';
import { WorkspaceService } from './core/services/workspace.service';
import { AppState } from './core/state/app.state';
import { UniversePickerComponent } from './features/universe-picker/universe-picker.component';
import { ConnectionsGraphComponent } from './features/connections/connections-graph.component';
import { WritingEditorComponent } from './features/writing/writing-editor.component';
import { AppShellComponent } from './shell/app-shell/app-shell.component';
import { ContextualInspectorComponent } from './shell/contextual-inspector/contextual-inspector.component';
import { TitlebarComponent } from './shell/titlebar/titlebar.component';
import { SidebarNavItem, UniverseSidebarComponent } from './shell/universe-sidebar/universe-sidebar.component';

interface StoredOnlineShare {
  id: string;
  revokeToken: string;
  expiresAt: string;
  title: string;
  apiUrl: string;
}

type EntityHubType = 'Personagem' | 'Lugar' | 'Evento' | 'Objeto' | 'Organização';

interface GlobalSearchResult {
  id: string;
  kind: 'story' | 'book' | 'chapter' | 'entity' | 'timeline' | 'planning';
  label: string;
  context: string;
  icon: string;
}

interface WritingCharacterInsight {
  entity: Entity;
  mentionedInCurrent: boolean;
  firstOccurrence: MentionOccurrence | null;
  dialogueSnippets: string[];
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    DragDropModule,
    AppShellComponent,
    TitlebarComponent,
    UniverseSidebarComponent,
    ContextualInspectorComponent,
    UniversePickerComponent,
    ConnectionsGraphComponent,
    WritingEditorComponent,
  ],
  templateUrl: './app.html',
  styleUrl: './app.css',
})
export class App implements OnInit, OnDestroy {
  readonly Math = Math;
  readonly appState = inject(AppState);
  readonly theme = inject(ThemeService);
  private readonly db = inject(DatabaseService);
  private readonly universeService = inject(UniverseService);
  private readonly storyService = inject(StoryService);
  private readonly bookService = inject(BookService);
  private readonly attachmentService = inject(AttachmentService);
  private readonly chapterService = inject(ChapterService);
  private readonly entityService = inject(EntityService);
  private readonly mentionService = inject(MentionService);
  private readonly onlineShareService = inject(OnlineShareService);
  private readonly workspaceService = inject(WorkspaceService);
  private readonly syncService = inject(SyncService);
  private readonly updateService = inject(UpdateService);

  readonly searchQuery = signal('');
  readonly activeNav = signal('inicio');
  readonly universes = signal<UniverseWithStats[]>([]);
  readonly stories = signal<Story[]>([]);
  readonly books = signal<Book[]>([]);
  readonly universeBooks = signal<BookOption[]>([]);
  readonly chapters = signal<Chapter[]>([]);
  readonly universeChapters = signal<ChapterOption[]>([]);
  readonly entities = signal<Entity[]>([]);
  readonly mentionOccurrences = signal<MentionOccurrence[]>([]);
  readonly timeline = signal<TimelineEvent[]>([]);
  readonly planning = signal<PlanningItem[]>([]);
  readonly relations = signal<RelationCard[]>([]);
  readonly history = signal<HistoryEntry[]>([]);
  readonly activeStory = signal<Story | null>(null);
  readonly activeBook = signal<Book | null>(null);
  readonly activeChapter = signal<Chapter | null>(null);
  readonly activeEntity = signal<EntityWithDetails | null>(null);
  readonly entityGallery = signal<Attachment[]>([]);
  readonly editorTitle = signal('');
  readonly editorContent = signal('');
  readonly isLoading = signal(true);
  readonly isSaving = signal(false);
  readonly isFocusMode = signal(false);
  readonly saveMessage = signal('');
  readonly errorMessage = signal('');
  readonly infoMessage = signal('');
  readonly syncBusy = signal(false);
  readonly shareBusy = signal(false);
  readonly shareServerState = signal<'idle' | 'checking' | 'online' | 'error'>('idle');
  readonly shareServerMessage = signal('');
  readonly shareLink = signal('');
  readonly shareExpiresAt = signal('');
  readonly onlineShares = signal<StoredOnlineShare[]>(this.readStoredShares());
  readonly updateBusy = signal(false);
  readonly updateProgress = signal(0);
  readonly updatePhase = signal<'idle' | 'checking' | 'available' | 'downloading' | 'current' | 'error'>('idle');
  readonly updateInfo = signal<AppUpdateInfo>({ currentVersion: '0.5.0', availableVersion: null, notes: '', publishedAt: null });
  readonly updateError = signal('');
  readonly updatePromptDismissed = signal(false);
  readonly syncStatus = signal<SyncServerStatus>({ running: false, address: null, pairing_code: null, device_name: 'Meu computador' });
  readonly lastOpenedUniverseId = signal<string | null>(localStorage.getItem('narrahub.lastUniverseId'));
  readonly pendingDeleteUniverse = signal<UniverseWithStats | null>(null);
  readonly expandedStoryIds = signal<Set<string>>(new Set());
  readonly expandedBookIds = signal<Set<string>>(new Set());

  newUniverseName = '';
  newUniverseDesc = '';
  newUniverseCoverData = '';
  editingUniverseId: string | null = null;
  deleteUniverseConfirmation = '';
  newStoryName = '';
  newBookName = '';
  newChapterTitle = '';
  newEntityName = '';
  newEntityDescription = '';
  newEntityImageData = '';
  newEntityType: EntityType = 'Personagem';
  newTimelineTitle = '';
  newTimelineDate = '';
  newTimelineDescription = '';
  newTimelineEntityId = '';
  newTimelineDisplayDate = '';
  newTimelineSortKey = 0;
  newPlanningTitle = '';
  newPlanningDescription = '';
  newPlanningChapterId = '';
  newRelationSource = '';
  newRelationTarget = '';
  newRelationLabel = '';
  deviceName = localStorage.getItem('narrahub.deviceName') || 'Meu computador';
  remoteAddress = '';
  pairingCode = '';
  shareApiUrl = localStorage.getItem('narrahub.shareApiUrl') || '';
  shareExpiresInDays = 7;

  readonly planningStatuses: PlanningStatus[] = ['IDEIAS', 'PLANEJADO', 'ESCREVENDO', 'REVISAO', 'FINALIZADO'];
  readonly navItems: SidebarNavItem[] = [
    { id: 'inicio', label: 'Início', icon: '⌂', needsUniverse: false },
    { id: 'escrita', label: 'Escrita', icon: '✎', needsUniverse: true },
    { id: 'entidades', label: 'Entidades', icon: '♧', needsUniverse: true },
    { id: 'conexoes', label: 'Conexões', icon: '⌘', needsUniverse: true },
    { id: 'timeline', label: 'Timeline', icon: '◷', needsUniverse: true },
    { id: 'planejamento', label: 'Planejamento', icon: '☑', needsUniverse: true },
    { id: 'historico', label: 'Histórico', icon: '↶', needsUniverse: true },
    { id: 'configuracoes', label: 'Configurações', icon: '⚙', needsUniverse: false },
  ];

  readonly wordCount = computed(() => this.countWords(this.editorContent()));
  readonly recentEntities = computed(() => [...this.entities()].sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 8));
  readonly globalSearchResults = computed<GlobalSearchResult[]>(() => {
    if (this.appState.currentView() !== 'workspace') return [];
    const query = this.normalizeSearch(this.searchQuery());
    if (query.length < 2) return [];
    const matches = (value: string) => this.normalizeSearch(value).includes(query);
    const results: GlobalSearchResult[] = [];
    for (const story of this.stories()) if (matches(`${story.name} ${story.description}`)) results.push({ id: story.id, kind: 'story', label: story.name, context: 'História', icon: '⌂' });
    for (const book of this.universeBooks()) if (matches(`${book.name} ${book.description} ${book.story_name}`)) results.push({ id: book.id, kind: 'book', label: book.name, context: `Livro · ${book.story_name}`, icon: '▱' });
    for (const chapter of this.universeChapters()) if (matches(`${chapter.title} ${chapter.content} ${chapter.book_name} ${chapter.story_name}`)) results.push({ id: chapter.id, kind: 'chapter', label: chapter.title, context: `${chapter.story_name} · ${chapter.book_name}`, icon: '▤' });
    for (const entity of this.entities()) if (matches(`${entity.name} ${entity.description} ${entity.type}`)) results.push({ id: entity.id, kind: 'entity', label: entity.name, context: entity.type, icon: entity.name.charAt(0).toUpperCase() });
    for (const event of this.timeline()) if (matches(`${event.title} ${event.description} ${event.display_date || event.start_date}`)) results.push({ id: event.id, kind: 'timeline', label: event.title, context: 'Linha do tempo', icon: '◷' });
    for (const item of this.planning()) if (matches(`${item.title} ${item.description} ${item.status}`)) results.push({ id: item.id, kind: 'planning', label: item.title, context: `Planejamento · ${item.status}`, icon: '☑' });
    return results.slice(0, 24);
  });
  readonly writingCharacters = computed<WritingCharacterInsight[]>(() => {
    const paragraphs = this.contentParagraphs(this.editorContent());
    return this.entities().filter((entity) => entity.type === 'Personagem').map((entity) => {
      const matching = paragraphs.filter((paragraph) => this.textMentionsEntity(paragraph, entity.name));
      const dialogueSnippets = matching.filter((paragraph) => this.looksLikeDialogue(paragraph, entity.name)).slice(0, 3);
      const firstOccurrence = this.mentionOccurrences().find((mention) => mention.entity_id === entity.id) ?? null;
      return { entity, mentionedInCurrent: matching.length > 0, firstOccurrence, dialogueSnippets };
    }).filter((insight) => insight.mentionedInCurrent || insight.firstOccurrence);
  });

  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private infoTimer: ReturnType<typeof setTimeout> | null = null;
  private workspaceEpoch = 0;

  async ngOnInit(): Promise<void> {
    if (!isTauri()) { this.isLoading.set(false); return; }
    try {
      await this.db.init();
      await this.loadUniverses();
      this.syncStatus.set(await this.syncService.status());
      const currentVersion = await this.updateService.currentVersion();
      this.updateInfo.update((info) => ({ ...info, currentVersion }));
      if (await this.updateService.isConfigured()) setTimeout(() => void this.checkForUpdates(true), 1800);
    } catch (error) {
      this.reportError('Não foi possível abrir o banco local do NarraHub.', error);
    } finally { this.isLoading.set(false); }
  }

  ngOnDestroy(): void {
    if (this.saveTimer) clearTimeout(this.saveTimer);
    if (this.infoTimer) clearTimeout(this.infoTimer);
    this.updateService.dispose();
  }

  @HostListener('document:keydown.control.k', ['$event'])
  focusSearch(event: Event): void {
    event.preventDefault();
    document.querySelector<HTMLInputElement>('.nh-global-search input')?.focus();
  }

  @HostListener('document:keydown.escape')
  clearSearch(): void { this.searchQuery.set(''); }

  async minimizeWindow(): Promise<void> { if (isTauri()) await getCurrentWindow().minimize(); }
  async toggleMaximizeWindow(): Promise<void> { if (isTauri()) await getCurrentWindow().toggleMaximize(); }
  async closeWindow(): Promise<void> { await this.saveChapterNow(); if (isTauri()) await getCurrentWindow().close(); }
  async toggleFullscreen(): Promise<void> { if (isTauri()) { const win = getCurrentWindow(); await win.setFullscreen(!(await win.isFullscreen())); } }
  async loadUniverses(): Promise<void> { this.universes.set(await this.universeService.list()); }

  async selectNav(item: SidebarNavItem): Promise<void> {
    if (item.needsUniverse && !this.appState.activeUniverse()) {
      this.showInfo('Selecione ou crie um universo para abrir esta área.'); return;
    }
    await this.saveChapterNow();
    this.activeNav.set(item.id);
    if (item.id === 'inicio') { await this.returnToLibrary(); return; }
    if (item.id === 'ajuda') { this.showInfo('Ajuda e feedback serão conectados ao fluxo nativo em uma próxima fase.'); return; }
    if (item.id === 'configuracoes') { this.appState.openSettings(); return; }
    if (item.id === 'escrita') this.appState.openEditor();
    else if (item.id === 'entidades') this.appState.openEntityList(null);
    else if (item.id === 'conexoes') { this.appState.openGraph(); await this.loadRelations(); }
    else if (item.id === 'timeline') { this.appState.openTimeline(); await this.loadTimeline(); }
    else if (item.id === 'planejamento') { this.appState.openPlanning(); await this.loadPlanning(); }
    else if (item.id === 'historico') { this.appState.openHistory(); await this.loadHistory(); }
  }

  async createUniverse(): Promise<void> {
    const name = this.newUniverseName.trim(); if (!name) return;
    try {
      if (this.editingUniverseId) {
        await this.universeService.update(this.editingUniverseId, { name, description: this.newUniverseDesc.trim(), cover_image: this.newUniverseCoverData });
        const active = this.appState.activeUniverse();
        if (active?.id === this.editingUniverseId) {
          this.appState.activeUniverse.set({ ...active, name, description: this.newUniverseDesc.trim(), cover_image: this.newUniverseCoverData, updated_at: this.db.now() });
        }
        this.resetUniverseForm(); this.appState.closeModal(); await this.loadUniverses(); this.showInfo('Universo atualizado.');
        return;
      }
      const created = await this.universeService.create(name, this.newUniverseDesc.trim());
      if (this.newUniverseCoverData) await this.universeService.update(created.id, { cover_image: this.newUniverseCoverData });
      this.resetUniverseForm(); this.appState.closeModal(); await this.loadUniverses();
      const universe = this.universes().find((item) => item.id === created.id); if (universe) await this.openUniverse(universe);
    } catch (error) { this.reportError('Não foi possível salvar o universo.', error); }
  }

  beginCreateUniverse(): void { this.resetUniverseForm(); this.appState.openModal('new-universe'); }

  beginEditUniverse(universe: UniverseWithStats): void {
    this.editingUniverseId = universe.id; this.newUniverseName = universe.name; this.newUniverseDesc = universe.description; this.newUniverseCoverData = universe.cover_image;
    this.appState.openModal('new-universe');
  }

  requestDeleteUniverse(universe: UniverseWithStats): void {
    this.pendingDeleteUniverse.set(universe); this.deleteUniverseConfirmation = ''; this.appState.openModal('delete-universe');
  }

  async openUniverse(universe: UniverseWithStats): Promise<void> {
    await this.saveChapterNow();
    this.workspaceEpoch += 1; this.resetWorkspaceData();
    localStorage.setItem('narrahub.lastUniverseId', universe.id); this.lastOpenedUniverseId.set(universe.id);
    this.searchQuery.set(''); this.appState.openUniverse(universe); this.activeNav.set('escrita'); await this.loadWorkspaceData();
  }

  async confirmDeleteUniverse(): Promise<void> {
    const universe = this.pendingDeleteUniverse();
    if (!universe || this.deleteUniverseConfirmation.trim() !== universe.name) return;
    try {
      await this.universeService.delete(universe.id); this.appState.closeModal(); this.pendingDeleteUniverse.set(null); await this.loadUniverses();
      if (this.lastOpenedUniverseId() === universe.id) { localStorage.removeItem('narrahub.lastUniverseId'); this.lastOpenedUniverseId.set(null); }
      if (this.appState.activeUniverseId() === universe.id) await this.returnToLibrary();
      this.showInfo('Universo excluído do banco local.');
    } catch (error) { this.reportError('Não foi possível excluir o universo.', error); }
  }

  async returnToLibrary(): Promise<void> {
    await this.saveChapterNow(); this.workspaceEpoch += 1; this.appState.goHome(); this.activeNav.set('inicio'); this.searchQuery.set(''); this.resetWorkspaceData();
  }

  openSettings(): void { this.searchQuery.set(''); this.activeNav.set('configuracoes'); this.appState.openSettings(); }

  openShareModal(): void {
    if (!this.activeChapter()) { this.showInfo('Abra um capítulo antes de compartilhar.'); return; }
    this.shareLink.set(''); this.shareExpiresAt.set(''); this.appState.openModal('share-content');
  }

  async loadWorkspaceData(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const epoch = this.workspaceEpoch;
    try {
      const [stories, books, entities, chapters, timeline, planning] = await Promise.all([
        this.storyService.listByUniverse(id),
        this.bookService.listByUniverse(id),
        this.entityService.listByUniverse(id),
        this.chapterService.listByUniverse(id),
        this.workspaceService.listTimeline(id),
        this.workspaceService.listPlanning(id),
      ]);
      if (epoch !== this.workspaceEpoch || this.appState.activeUniverseId() !== id) return;
      this.stories.set(stories); this.universeBooks.set(books); this.entities.set(entities); this.universeChapters.set(chapters); this.timeline.set(timeline); this.planning.set(planning);
      this.expandedStoryIds.set(new Set(stories.slice(0, 1).map((story) => story.id)));
      this.expandedBookIds.set(new Set(books.slice(0, 1).map((book) => book.id)));
      if (this.stories().length) await this.selectStory(this.stories()[0]); else this.clearWritingSelection();
      void this.rebuildMentionIndex(id, chapters, entities);
    } catch (error) { this.reportError('Não foi possível carregar os dados do universo.', error); }
  }

  async createStory(): Promise<void> {
    const id = this.appState.activeUniverseId(); const name = this.newStoryName.trim(); if (!id || !name) return;
    try {
      const story = await this.storyService.create(id, name); this.newStoryName = ''; this.appState.closeModal();
      this.stories.set(await this.storyService.listByUniverse(id)); this.setExpanded(this.expandedStoryIds, story.id, true); await this.selectStory(story); await this.refreshUniverseStats();
    } catch (error) { this.reportError('Não foi possível criar a história.', error); }
  }

  async selectStory(story: Story): Promise<void> {
    await this.saveChapterNow();
    const universeId = this.appState.activeUniverseId();
    const books = await this.bookService.listByStory(story.id);
    if (!universeId || this.appState.activeUniverseId() !== universeId || !this.stories().some((item) => item.id === story.id)) return;
    this.activeStory.set(story); this.appState.activeStoryId.set(story.id); this.books.set(books); this.setExpanded(this.expandedStoryIds, story.id, true);
    if (this.books().length) await this.selectBook(this.books()[0]);
    else { this.activeBook.set(null); this.activeChapter.set(null); this.chapters.set([]); this.editorTitle.set(''); this.editorContent.set(''); }
  }

  async createBook(): Promise<void> {
    const story = this.activeStory(); const name = this.newBookName.trim(); if (!story || !name) return;
    try {
      const book = await this.bookService.create(story.id, name); this.newBookName = ''; this.appState.closeModal();
      this.books.set(await this.bookService.listByStory(story.id)); await this.refreshUniverseBooks(); this.setExpanded(this.expandedBookIds, book.id, true); await this.selectBook(book); await this.refreshUniverseStats();
    } catch (error) { this.reportError('Não foi possível criar o livro.', error); }
  }

  async selectBook(book: Book): Promise<void> {
    await this.saveChapterNow();
    const storyId = this.activeStory()?.id;
    const chapters = await this.chapterService.listByBook(book.id);
    if (!storyId || this.activeStory()?.id !== storyId || !this.books().some((item) => item.id === book.id)) return;
    this.activeBook.set(book); this.appState.activeBookId.set(book.id); this.chapters.set(chapters); this.setExpanded(this.expandedBookIds, book.id, true);
    if (this.chapters().length) await this.selectChapter(this.chapters()[0]); else { this.activeChapter.set(null); this.editorTitle.set(''); this.editorContent.set(''); }
  }

  async createChapter(): Promise<void> {
    const book = this.activeBook(); const title = this.newChapterTitle.trim(); if (!book || !title) return;
    try {
      const chapter = await this.chapterService.create(book.id, title); this.newChapterTitle = ''; this.appState.closeModal();
      this.chapters.set(await this.chapterService.listByBook(book.id)); await this.selectChapter(chapter); await this.refreshUniverseChapters(); await this.refreshUniverseStats();
    } catch (error) { this.reportError('Não foi possível criar o capítulo.', error); }
  }

  async selectChapter(chapter: Chapter): Promise<void> {
    if (this.activeChapter()?.id === chapter.id) return;
    await this.saveChapterNow(); this.activeChapter.set(chapter); this.appState.activeChapterId.set(chapter.id);
    this.editorTitle.set(chapter.title); this.editorContent.set(chapter.content || ''); this.saveMessage.set('Salvo');
  }

  booksForStory(storyId: string): BookOption[] { return this.universeBooks().filter((book) => book.story_id === storyId); }
  chaptersForBook(bookId: string): ChapterOption[] { return this.universeChapters().filter((chapter) => chapter.book_id === bookId); }
  isStoryExpanded(id: string): boolean { return this.expandedStoryIds().has(id); }
  isBookExpanded(id: string): boolean { return this.expandedBookIds().has(id); }
  toggleStory(story: Story, event: Event): void { event.stopPropagation(); this.setExpanded(this.expandedStoryIds, story.id, !this.isStoryExpanded(story.id)); }
  toggleBook(book: Book, event: Event): void { event.stopPropagation(); this.setExpanded(this.expandedBookIds, book.id, !this.isBookExpanded(book.id)); }

  async openBookOption(book: BookOption): Promise<void> {
    const story = this.stories().find((candidate) => candidate.id === book.story_id); if (!story) return;
    await this.selectStory(story);
    const selected = this.books().find((candidate) => candidate.id === book.id); if (selected) await this.selectBook(selected);
    this.activeNav.set('escrita'); this.appState.openEditor();
  }

  async openChapterOption(option: ChapterOption): Promise<void> {
    const story = this.stories().find((candidate) => candidate.id === option.story_id); if (!story) return;
    await this.selectStory(story);
    const book = this.books().find((candidate) => candidate.id === option.book_id); if (!book) return;
    await this.selectBook(book);
    const chapter = this.chapters().find((candidate) => candidate.id === option.id); if (chapter) await this.selectChapter(chapter);
    this.activeNav.set('escrita'); this.appState.openEditor(option.id);
  }

  async selectTreeChapter(option: ChapterOption): Promise<void> { await this.openChapterOption(option); }

  async moveTreeChapter(chapter: ChapterOption, direction: -1 | 1, event: Event): Promise<void> {
    event.stopPropagation();
    const items = this.chaptersForBook(chapter.book_id); const index = items.findIndex((item) => item.id === chapter.id); const next = index + direction;
    if (index < 0 || next < 0 || next >= items.length) return;
    [items[index], items[next]] = [items[next], items[index]];
    await this.chapterService.reorder(chapter.book_id, items.map((item) => item.id)); await this.refreshUniverseChapters();
    if (this.activeBook()?.id === chapter.book_id) this.chapters.set(await this.chapterService.listByBook(chapter.book_id));
  }

  async dropTreeChapter(event: CdkDragDrop<ChapterOption[]>, book: BookOption): Promise<void> {
    if (event.previousIndex === event.currentIndex) return;
    const items = [...event.container.data]; moveItemInArray(items, event.previousIndex, event.currentIndex);
    await this.chapterService.reorder(book.id, items.map((item) => item.id)); await this.refreshUniverseChapters();
    if (this.activeBook()?.id === book.id) this.chapters.set(await this.chapterService.listByBook(book.id));
  }

  async onBookCoverSelected(event: Event, book: Book): Promise<void> {
    const input = event.target as HTMLInputElement; const file = input.files?.[0]; input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/') || file.size > 8 * 1024 * 1024) { this.showInfo('Escolha uma imagem de até 8 MB.'); return; }
    const cover_image = await this.fileToDataUrl(file); await this.bookService.update(book.id, { cover_image });
    this.books.update((items) => items.map((item) => item.id === book.id ? { ...item, cover_image } : item));
    this.universeBooks.update((items) => items.map((item) => item.id === book.id ? { ...item, cover_image } : item));
    if (this.activeBook()?.id === book.id) this.activeBook.update((active) => active ? { ...active, cover_image } : active);
    this.showInfo('Capa do livro atualizada.');
  }

  async moveChapter(chapter: Chapter, direction: -1 | 1, event: Event): Promise<void> {
    event.stopPropagation(); const items = [...this.chapters()]; const index = items.findIndex((item) => item.id === chapter.id); const next = index + direction;
    if (index < 0 || next < 0 || next >= items.length || !this.activeBook()) return;
    [items[index], items[next]] = [items[next], items[index]];
    await this.chapterService.reorder(this.activeBook()!.id, items.map((item) => item.id)); this.chapters.set(items);
  }

  async dropChapter(event: CdkDragDrop<Chapter[]>): Promise<void> {
    const book = this.activeBook(); if (!book || event.previousIndex === event.currentIndex) return;
    const items = [...this.chapters()]; moveItemInArray(items, event.previousIndex, event.currentIndex);
    this.chapters.set(items);
    await this.chapterService.reorder(book.id, items.map((item) => item.id));
  }

  onEditorInput(content: string): void { this.editorContent.set(content); this.queueChapterSave(); }
  onTitleInput(title: string): void { this.editorTitle.set(title); this.queueChapterSave(); }
  applyFormat(prefix: string, suffix = prefix): void {
    const editor = document.querySelector<HTMLTextAreaElement>('.chapter-editor'); if (!editor) return;
    const start = editor.selectionStart; const end = editor.selectionEnd; const content = this.editorContent(); const selection = content.slice(start, end) || 'texto';
    this.onEditorInput(`${content.slice(0, start)}${prefix}${selection}${suffix}${content.slice(end)}`);
    queueMicrotask(() => { editor.focus(); editor.setSelectionRange(start + prefix.length, start + prefix.length + selection.length); });
  }
  private queueChapterSave(): void {
    if (!this.activeChapter()) return; this.saveMessage.set('Alterações pendentes'); if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => void this.saveChapterNow(), 700);
  }
  async saveChapterNow(): Promise<void> {
    if (this.saveTimer) { clearTimeout(this.saveTimer); this.saveTimer = null; }
    const chapter = this.activeChapter(); if (!chapter || this.saveMessage() === 'Salvo') return;
    const title = this.editorTitle().trim() || 'Capítulo sem título'; const content = this.editorContent(); const words = this.countWords(content); this.isSaving.set(true);
    try {
      if (title !== chapter.title) await this.chapterService.updateTitle(chapter.id, title);
      if (content !== chapter.content || words !== chapter.word_count) await this.chapterService.updateContent(chapter.id, content, words);
      const updated = { ...chapter, title, content, word_count: words, updated_at: this.db.now() };
      this.activeChapter.set(updated); this.chapters.update((items) => items.map((item) => item.id === updated.id ? updated : item));
      this.universeChapters.update((items) => items.map((item) => item.id === updated.id ? { ...item, ...updated } : item));
      await this.mentionService.syncChapterMentions(chapter.id, this.entityIdsInContent(content, this.entities()));
      this.saveMessage.set('Salvo'); await Promise.all([this.refreshUniverseStats(), this.refreshMentionOccurrences()]);
    } catch (error) { this.saveMessage.set('Erro ao salvar'); this.reportError('O capítulo não foi salvo.', error); }
    finally { this.isSaving.set(false); }
  }

  async createEntity(): Promise<void> {
    const id = this.appState.activeUniverseId(); const name = this.newEntityName.trim(); if (!id || !name) return;
    try {
      const entity = await this.entityService.create(id, this.newEntityType, name, this.newEntityDescription.trim());
      if (this.newEntityImageData) await this.entityService.update(entity.id, { image: this.newEntityImageData });
      this.newEntityName = ''; this.newEntityDescription = ''; this.newEntityImageData = ''; this.appState.closeModal();
      this.entities.set(await this.entityService.listByUniverse(id));
      const created = this.entities().find((item) => item.id === entity.id);
      if (created) await this.openEntitySheet(created);
      await this.refreshUniverseStats();
    } catch (error) { this.reportError('Não foi possível criar a entidade.', error); }
  }
  beginCreateEntity(): void {
    this.newEntityType = (this.appState.sidebarEntityFilter() || 'Personagem') as EntityType;
    this.newEntityName = ''; this.newEntityDescription = ''; this.newEntityImageData = ''; this.appState.openModal('new-entity');
  }
  selectEntityTab(type: EntityHubType | null): void {
    this.activeEntity.set(null); this.entityGallery.set([]); this.appState.openEntityList(type);
  }
  entityCreateLabel(): string {
    const type = this.appState.sidebarEntityFilter() || 'entidade';
    return `Novo ${type.toLocaleLowerCase('pt-BR')}`;
  }
  async openEntitySheet(entity: Entity): Promise<void> {
    try {
      const universeId = this.appState.activeUniverseId(); if (!universeId || entity.universe_id !== universeId) return;
      const [details, gallery] = await Promise.all([
        this.entityService.getWithDetails(entity.id),
        this.attachmentService.list(universeId, 'entity', entity.id),
      ]);
      if (this.appState.activeUniverseId() !== universeId) return;
      this.activeEntity.set(details); this.entityGallery.set(gallery); this.appState.openEntitySheet(entity.id);
    }
    catch (error) { this.reportError('Não foi possível abrir a ficha.', error); }
  }
  visibleEntities(): Entity[] {
    const filter = this.appState.sidebarEntityFilter(); const query = this.searchQuery().trim().toLocaleLowerCase('pt-BR');
    return this.entities().filter((entity) =>
      (!filter || entity.type === filter) &&
      (!query || `${entity.name} ${entity.description} ${entity.type}`.toLocaleLowerCase('pt-BR').includes(query)),
    );
  }
  entitySectionTitle(): string {
    const filter = this.appState.sidebarEntityFilter();
    if (!filter) return 'Entidades';
    return ({ Personagem: 'Personagens', Lugar: 'Lugares', Evento: 'Eventos', Objeto: 'Objetos', 'Organização': 'Organizações' } as Record<string, string>)[filter] ?? filter;
  }
  entityTypeCount(type: EntityHubType | null): number { return type ? this.entities().filter((entity) => entity.type === type).length : this.entities().length; }

  async openGlobalSearchResult(result: GlobalSearchResult): Promise<void> {
    this.searchQuery.set('');
    if (result.kind === 'story') {
      const story = this.stories().find((item) => item.id === result.id); if (story) { await this.selectStory(story); this.activeNav.set('escrita'); this.appState.openEditor(); }
    } else if (result.kind === 'book') {
      const book = this.universeBooks().find((item) => item.id === result.id); if (book) await this.openBookOption(book);
    } else if (result.kind === 'chapter') {
      const chapter = this.universeChapters().find((item) => item.id === result.id); if (chapter) await this.openChapterOption(chapter);
    } else if (result.kind === 'entity') {
      const entity = this.entities().find((item) => item.id === result.id); if (entity) { this.activeNav.set('entidades'); this.appState.openEntityList(entity.type); await this.openEntitySheet(entity); }
    } else if (result.kind === 'timeline') {
      this.activeNav.set('timeline'); this.appState.openTimeline(); queueMicrotask(() => document.querySelector<HTMLElement>(`[data-timeline-id="${result.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' }));
    } else {
      this.activeNav.set('planejamento'); this.appState.openPlanning(); queueMicrotask(() => document.querySelector<HTMLElement>(`[data-planning-id="${result.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' }));
    }
  }

  patchActiveEntity(field: 'name' | 'description' | 'canon_status', value: string): void {
    this.activeEntity.update((entity) => entity ? { ...entity, [field]: value } : entity);
  }

  async onImageSelected(event: Event, target: 'universe' | 'entity-new' | 'entity-active'): Promise<void> {
    const input = event.target as HTMLInputElement; const file = input.files?.[0]; input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) { this.showInfo('Escolha um arquivo de imagem.'); return; }
    if (file.size > 8 * 1024 * 1024) { this.showInfo('A imagem deve ter no máximo 8 MB.'); return; }
    const dataUrl = await this.fileToDataUrl(file);
    if (target === 'universe') this.newUniverseCoverData = dataUrl;
    else if (target === 'entity-new') this.newEntityImageData = dataUrl;
    else {
      const entity = this.activeEntity(); if (!entity) return;
      await this.entityService.update(entity.id, { image: dataUrl });
      this.activeEntity.set({ ...entity, image: dataUrl });
      this.entities.update((items) => items.map((item) => item.id === entity.id ? { ...item, image: dataUrl } : item));
      this.showInfo('Imagem principal atualizada.');
    }
  }

  async updateActiveEntity(): Promise<void> {
    const entity = this.activeEntity(); if (!entity) return;
    await this.entityService.update(entity.id, { name: entity.name.trim(), description: entity.description, canon_status: entity.canon_status });
    for (const attribute of entity.attributes) await this.entityService.setAttribute(entity.id, attribute.key, attribute.value);
    this.entities.update((items) => items.map((item) => item.id === entity.id ? { ...item, name: entity.name, description: entity.description, canon_status: entity.canon_status } : item));
    this.showInfo('Ficha salva.');
  }

  async addEntityGalleryImages(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement; const files = [...(input.files ?? [])]; input.value = '';
    const entity = this.activeEntity(); const universeId = this.appState.activeUniverseId();
    if (!entity || !universeId || entity.universe_id !== universeId || !files.length) return;
    const accepted = files.filter((file) => file.type.startsWith('image/') && file.size <= 8 * 1024 * 1024).slice(0, 12);
    if (accepted.length !== files.length) this.showInfo('Algumas imagens foram ignoradas: use até 12 arquivos de imagem com no máximo 8 MB cada.');
    for (const file of accepted) await this.attachmentService.create(universeId, 'entity', entity.id, await this.fileToDataUrl(file), file.name);
    this.entityGallery.set(await this.attachmentService.list(universeId, 'entity', entity.id));
  }

  async deleteEntityGalleryImage(id: string): Promise<void> {
    await this.attachmentService.delete(id);
    this.entityGallery.update((items) => items.filter((item) => item.id !== id));
  }

  async loadRelations(): Promise<void> { const id = this.appState.activeUniverseId(); if (!id) return; const data = await this.workspaceService.listRelations(id); if (this.appState.activeUniverseId() === id) this.relations.set(data); }
  async createRelation(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id || !this.newRelationSource || !this.newRelationTarget || !this.newRelationLabel.trim()) return;
    if (this.newRelationSource === this.newRelationTarget) { this.showInfo('Escolha duas entidades diferentes.'); return; }
    await this.workspaceService.createRelation(id, this.newRelationSource, this.newRelationTarget, this.newRelationLabel.trim());
    this.newRelationSource = ''; this.newRelationTarget = ''; this.newRelationLabel = ''; this.appState.closeModal(); await this.loadRelations();
  }
  async deleteRelation(id: string): Promise<void> { await this.workspaceService.deleteRelation(id); await this.loadRelations(); }

  async loadTimeline(): Promise<void> { const id = this.appState.activeUniverseId(); if (!id) return; const data = await this.workspaceService.listTimeline(id); if (this.appState.activeUniverseId() === id) this.timeline.set(data); }
  async createTimeline(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id || !this.newTimelineTitle.trim() || (!this.newTimelineDate && !this.newTimelineDisplayDate.trim())) return;
    await this.workspaceService.createTimeline(id, this.newTimelineTitle.trim(), this.newTimelineDate || '0000-01-01', this.newTimelineDescription.trim(), this.newTimelineEntityId || null, this.newTimelineDisplayDate.trim(), Number(this.newTimelineSortKey) || 0);
    this.newTimelineTitle = ''; this.newTimelineDate = ''; this.newTimelineDescription = ''; this.newTimelineEntityId = ''; this.newTimelineDisplayDate = ''; this.newTimelineSortKey = 0; this.appState.closeModal(); await this.loadTimeline();
  }
  async deleteTimeline(id: string): Promise<void> { await this.workspaceService.deleteTimeline(id); await this.loadTimeline(); }

  async loadPlanning(): Promise<void> { const id = this.appState.activeUniverseId(); if (!id) return; const data = await this.workspaceService.listPlanning(id); if (this.appState.activeUniverseId() === id) this.planning.set(data); }
  planningByStatus(status: PlanningStatus): PlanningItem[] { return this.planning().filter((item) => item.status === status); }
  async createPlanning(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const linkedChapter = this.universeChapters().find((chapter) => chapter.id === this.newPlanningChapterId);
    const title = this.newPlanningTitle.trim() || linkedChapter?.title || '';
    if (!title) return;
    await this.workspaceService.createPlanning(id, title, this.newPlanningDescription.trim(), linkedChapter?.id || null);
    this.newPlanningTitle = ''; this.newPlanningDescription = ''; this.newPlanningChapterId = ''; this.appState.closeModal(); await this.loadPlanning();
  }
  async movePlanning(item: PlanningItem, direction: -1 | 1): Promise<void> {
    const index = this.planningStatuses.indexOf(item.status) + direction; if (index < 0 || index >= this.planningStatuses.length) return;
    await this.workspaceService.movePlanning(item.id, this.planningStatuses[index]); await this.loadPlanning();
  }
  async dropPlanning(event: CdkDragDrop<PlanningItem[]>, targetStatus: PlanningStatus): Promise<void> {
    const item = event.item.data as PlanningItem;
    if (!item || item.status === targetStatus) return;
    await this.workspaceService.movePlanning(item.id, targetStatus); await this.loadPlanning();
  }
  async deletePlanning(id: string): Promise<void> { await this.workspaceService.deletePlanning(id); await this.loadPlanning(); }
  async loadHistory(): Promise<void> { const id = this.appState.activeUniverseId(); if (!id) return; const data = await this.workspaceService.listHistory(id); if (this.appState.activeUniverseId() === id) this.history.set(data); }

  async openPlanningChapter(item: PlanningItem): Promise<void> {
    const option = this.universeChapters().find((chapter) => chapter.id === item.chapter_id); if (!option) return;
    const story = this.stories().find((candidate) => candidate.id === option.story_id); if (!story) return;
    await this.selectStory(story);
    const book = this.books().find((candidate) => candidate.id === option.book_id); if (!book) return;
    await this.selectBook(book);
    const chapter = this.chapters().find((candidate) => candidate.id === option.id); if (chapter) await this.selectChapter(chapter);
    this.activeNav.set('escrita'); this.appState.openEditor(option.id);
  }

  setTheme(value: string): void { this.theme.setTheme(value as ThemePreference); }
  async saveShareServer(): Promise<void> {
    try {
      if (!this.shareApiUrl.trim()) {
        localStorage.removeItem('narrahub.shareApiUrl'); this.shareApiUrl = ''; this.shareServerState.set('idle'); this.shareServerMessage.set(''); this.showInfo('Servidor de compartilhamento removido.'); return;
      }
      this.shareApiUrl = this.onlineShareService.normalizeApiUrl(this.shareApiUrl);
      await this.testShareServer(true);
    } catch (error) { this.reportError('Endereço de compartilhamento inválido.', error); }
  }
  async testShareServer(persist = false): Promise<void> {
    if (!this.shareApiUrl.trim()) { this.shareServerState.set('error'); this.shareServerMessage.set('Informe primeiro o endereço HTTPS do servidor.'); return; }
    this.shareServerState.set('checking'); this.shareServerMessage.set('Verificando conexão segura…');
    try {
      this.shareApiUrl = this.onlineShareService.normalizeApiUrl(this.shareApiUrl);
      const health = await this.onlineShareService.health(this.shareApiUrl);
      if (persist) localStorage.setItem('narrahub.shareApiUrl', this.shareApiUrl);
      this.shareServerState.set('online'); this.shareServerMessage.set(health.encryption === 'client-side' ? 'Servidor online. A criptografia ocorre neste dispositivo.' : 'Servidor online.');
      this.showInfo(persist ? 'Servidor validado e salvo.' : 'Servidor de compartilhamento online.');
    } catch (error) {
      this.shareServerState.set('error'); this.shareServerMessage.set(error instanceof Error ? error.message : String(error));
      if (persist) localStorage.removeItem('narrahub.shareApiUrl');
      this.reportError('Não foi possível validar o servidor de compartilhamento.', error);
    }
  }
  async createOnlineShare(): Promise<void> {
    const chapter = this.activeChapter(); if (!chapter) return;
    if (!this.shareApiUrl.trim()) { this.showInfo('Configure o servidor em Configurações > Compartilhamento online.'); return; }
    this.errorMessage.set('');
    this.shareBusy.set(true);
    try {
      const health = await this.onlineShareService.health(this.shareApiUrl);
      this.shareServerState.set('online'); this.shareServerMessage.set(health.encryption === 'client-side' ? 'Servidor online. A criptografia ocorre neste dispositivo.' : 'Servidor online.');
      await this.saveChapterNow();
      const created = await this.onlineShareService.create(this.shareApiUrl, {
        version: 1,
        kind: 'chapter',
        title: this.editorTitle().trim() || chapter.title,
        content: this.editorContent(),
        universeName: this.appState.activeUniverse()?.name || '',
        storyName: this.activeStory()?.name || '',
        bookName: this.activeBook()?.name || '',
        sharedAt: new Date().toISOString(),
      }, Number(this.shareExpiresInDays));
      this.shareLink.set(created.url); this.shareExpiresAt.set(created.expiresAt);
      this.rememberShare(created.id, created.revokeToken, created.expiresAt, this.editorTitle().trim() || chapter.title, this.shareApiUrl);
      const copied = await this.copyShareLink(false);
      this.showInfo(copied ? 'Link criptografado criado e copiado.' : 'Link criptografado criado. Copie-o manualmente.');
    } catch (error) { this.reportError('Não foi possível criar o compartilhamento online.', error); }
    finally { this.shareBusy.set(false); }
  }
  async copyShareLink(showFeedback = true): Promise<boolean> {
    if (!this.shareLink()) return false;
    try {
      await navigator.clipboard.writeText(this.shareLink());
      if (showFeedback) this.showInfo('Link copiado.');
      return true;
    } catch (error) {
      console.warn('[NarraHub] Não foi possível copiar o link automaticamente.', error);
      if (showFeedback) this.showInfo('Não foi possível copiar automaticamente. Selecione o link manualmente.');
      return false;
    }
  }
  async revokeOnlineShare(share: StoredOnlineShare): Promise<void> {
    const apiUrl = share.apiUrl || this.shareApiUrl;
    if (!apiUrl) { this.showInfo('Configure o servidor usado para criar este link.'); return; }
    this.shareBusy.set(true); this.errorMessage.set('');
    try {
      await this.onlineShareService.revoke(apiUrl, share.id, share.revokeToken);
      const remaining = this.onlineShares().filter((item) => item.id !== share.id);
      this.onlineShares.set(remaining); this.persistStoredShares(remaining);
      this.showInfo('Compartilhamento revogado. O link não pode mais ser aberto.');
    } catch (error) { this.reportError('Não foi possível revogar o compartilhamento.', error); }
    finally { this.shareBusy.set(false); }
  }
  async checkForUpdates(silent = false): Promise<void> {
    if (!isTauri()) { if (!silent) this.showInfo('A atualização automática funciona somente no aplicativo instalado.'); return; }
    if (this.updateBusy()) return;
    if (!(await this.updateService.isConfigured())) {
      if (!silent) this.showInfo('Este build de desenvolvimento não possui um canal de atualização configurado.');
      return;
    }
    this.updateBusy.set(true); this.updatePhase.set('checking'); this.updateError.set(''); this.updateProgress.set(0);
    try {
      const info = await this.updateService.check(); this.updateInfo.set(info);
      this.updatePhase.set(info.availableVersion ? 'available' : 'current');
      if (info.availableVersion) this.updatePromptDismissed.set(false);
      if (!silent) this.showInfo(info.availableVersion ? `Versão ${info.availableVersion} disponível.` : 'O NarraHub está atualizado.');
    } catch (error) {
      this.updatePhase.set('error'); this.updateError.set(error instanceof Error ? error.message : String(error));
      if (!silent) this.reportError('Não foi possível verificar atualizações.', error);
    } finally { this.updateBusy.set(false); }
  }
  async installUpdate(): Promise<void> {
    if (!this.updateInfo().availableVersion || this.updateBusy()) return;
    this.updateBusy.set(true); this.updatePhase.set('downloading'); this.updateProgress.set(0); this.updateError.set('');
    try {
      await this.saveChapterNow();
      if (this.saveMessage() === 'Erro ao salvar') throw new Error('A atualização foi interrompida porque o capítulo atual não pôde ser salvo.');
      await this.updateService.downloadAndInstall((progress) => this.updateProgress.set(progress));
      await this.updateService.relaunch();
    } catch (error) {
      this.updatePhase.set('error'); this.updateError.set(error instanceof Error ? error.message : String(error));
      this.reportError('Não foi possível instalar a atualização.', error);
    } finally { this.updateBusy.set(false); }
  }
  dismissUpdatePrompt(): void { this.updatePromptDismissed.set(true); }
  saveDeviceName(): void { this.deviceName = this.deviceName.trim() || 'Meu computador'; localStorage.setItem('narrahub.deviceName', this.deviceName); this.showInfo('Nome do dispositivo salvo.'); }
  async startSync(): Promise<void> {
    if (!isTauri()) { this.showInfo('A sincronização de rede só funciona no aplicativo instalado.'); return; }
    this.syncBusy.set(true); try { this.saveDeviceName(); this.syncStatus.set(await this.syncService.start(this.deviceName)); }
    catch (error) { this.reportError('Não foi possível iniciar a sincronização.', error); } finally { this.syncBusy.set(false); }
  }
  async stopSync(): Promise<void> { this.syncBusy.set(true); try { this.syncStatus.set(await this.syncService.stop()); } catch (error) { this.reportError('Não foi possível parar a sincronização.', error); } finally { this.syncBusy.set(false); } }
  async connectSync(): Promise<void> {
    if (!isTauri()) { this.showInfo('A sincronização de rede só funciona no aplicativo instalado.'); return; }
    if (!this.remoteAddress.trim() || !/^\d{6}$/.test(this.pairingCode.trim())) { this.showInfo('Informe endereço e código de seis dígitos.'); return; }
    this.syncBusy.set(true);
    try {
      const result = await this.syncService.connect(this.remoteAddress.trim(), this.pairingCode.trim(), this.deviceName); await this.loadUniverses();
      this.showInfo(`Sincronizado com ${result.peer_name}: ${result.received} recebidos, ${result.sent} enviados, ${result.conflicts} conflitos.`);
    } catch (error) { this.reportError('A sincronização não foi concluída.', error); } finally { this.syncBusy.set(false); }
  }

  formatNumber(value: number): string { return value.toLocaleString('pt-BR'); }
  formatDate(value: string): string { if (!value) return 'Sem data'; const date = new Date(value.length === 10 ? `${value}T12:00:00` : value); return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' }); }
  private async refreshUniverseStats(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return; const stats = await this.universeService.getStats(id);
    this.universes.update((items) => items.map((item) => item.id === id ? { ...item, stats } : item));
    const active = this.appState.activeUniverse(); if (active) this.appState.activeUniverse.set({ ...active, stats });
  }
  private async refreshUniverseChapters(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const chapters = await this.chapterService.listByUniverse(id);
    if (this.appState.activeUniverseId() === id) this.universeChapters.set(chapters);
  }
  private async refreshUniverseBooks(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const books = await this.bookService.listByUniverse(id);
    if (this.appState.activeUniverseId() === id) this.universeBooks.set(books);
  }
  private async refreshMentionOccurrences(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const mentions = await this.mentionService.listByUniverse(id);
    if (this.appState.activeUniverseId() === id) this.mentionOccurrences.set(mentions);
  }
  private async rebuildMentionIndex(universeId: string, chapters: ChapterOption[], entities: Entity[]): Promise<void> {
    try {
      for (const chapter of chapters) {
        if (this.appState.activeUniverseId() !== universeId) return;
        await this.mentionService.syncChapterMentions(chapter.id, this.entityIdsInContent(chapter.content || '', entities));
      }
      await this.refreshMentionOccurrences();
    } catch (error) { console.warn('[NarraHub] Não foi possível atualizar o índice de menções.', error); }
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
    const normalizedText = this.normalizeSearch(text); const normalizedName = this.normalizeSearch(name);
    if (normalizedName.length < 2) return false;
    const escaped = normalizedName.replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
    return new RegExp(`(?:^|[^a-z0-9])${escaped}(?:$|[^a-z0-9])`, 'u').test(normalizedText);
  }
  private looksLikeDialogue(paragraph: string, name: string): boolean {
    const normalized = this.normalizeSearch(paragraph); const entityName = this.normalizeSearch(name);
    return /^[—–-]|[“”"]|\b(disse|perguntou|respondeu|sussurrou|gritou|falou)\b/u.test(normalized) || normalized.startsWith(`${entityName}:`);
  }
  private normalizeSearch(value: string): string { return value.normalize('NFD').replace(/[\u0300-\u036f]/gu, '').toLocaleLowerCase('pt-BR').trim(); }
  private setExpanded(target: typeof this.expandedStoryIds, id: string, expanded: boolean): void {
    target.update((current) => { const next = new Set(current); if (expanded) next.add(id); else next.delete(id); return next; });
  }
  private clearWritingSelection(): void { this.activeStory.set(null); this.activeBook.set(null); this.activeChapter.set(null); this.books.set([]); this.chapters.set([]); this.editorTitle.set(''); this.editorContent.set(''); }
  private resetWorkspaceData(): void { this.clearWritingSelection(); this.stories.set([]); this.universeBooks.set([]); this.universeChapters.set([]); this.entities.set([]); this.mentionOccurrences.set([]); this.timeline.set([]); this.planning.set([]); this.relations.set([]); this.history.set([]); this.activeEntity.set(null); this.entityGallery.set([]); this.expandedStoryIds.set(new Set()); this.expandedBookIds.set(new Set()); }
  private resetUniverseForm(): void { this.editingUniverseId = null; this.newUniverseName = ''; this.newUniverseDesc = ''; this.newUniverseCoverData = ''; }
  private fileToDataUrl(file: File): Promise<string> { return new Promise((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(reader.error ?? new Error('Falha ao ler a imagem.')); reader.readAsDataURL(file); }); }
  private rememberShare(id: string, revokeToken: string, expiresAt: string, title: string, apiUrl: string): void {
    const shares = [{ id, revokeToken, expiresAt, title, apiUrl }, ...this.onlineShares().filter((item) => item.id !== id)].slice(0, 50);
    this.onlineShares.set(shares); this.persistStoredShares(shares);
  }
  private readStoredShares(): StoredOnlineShare[] {
    try {
      const value = JSON.parse(localStorage.getItem('narrahub.onlineShares') || '[]');
      if (!Array.isArray(value)) return [];
      return value.filter((item) => item && /^[A-Za-z0-9_-]{16}$/u.test(item.id) && typeof item.revokeToken === 'string' && typeof item.expiresAt === 'string')
        .map((item) => ({ ...item, title: typeof item.title === 'string' ? item.title : 'Capítulo compartilhado', apiUrl: typeof item.apiUrl === 'string' ? item.apiUrl : '' }));
    } catch { return []; }
  }
  private persistStoredShares(shares: StoredOnlineShare[]): void {
    localStorage.setItem('narrahub.onlineShares', JSON.stringify(shares));
  }
  private countWords(content: string): number { const normalized = content.replace(/<[^>]+>/g, ' ').trim(); return normalized ? normalized.split(/\s+/u).length : 0; }
  private showInfo(message: string): void { this.infoMessage.set(message); if (this.infoTimer) clearTimeout(this.infoTimer); this.infoTimer = setTimeout(() => this.infoMessage.set(''), 4200); }
  private reportError(message: string, error: unknown): void { console.error(`[NarraHub] ${message}`, error); this.errorMessage.set(`${message} ${error instanceof Error ? error.message : String(error)}`); }
}
