import { CommonModule } from '@angular/common';
import { Component, HostListener, OnDestroy, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { CdkDragDrop, DragDropModule, moveItemInArray } from '@angular/cdk/drag-drop';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Attachment, Book, Chapter, ChapterOption, Entity, EntityType, EntityWithDetails, HistoryEntry, PlanningItem,
  PlanningStatus, RelationCard, Story, SyncServerStatus, TimelineEvent, UniverseWithStats,
} from './core/models';
import { BookService } from './core/services/book.service';
import { AttachmentService } from './core/services/attachment.service';
import { ChapterService } from './core/services/chapter.service';
import { DatabaseService } from './core/services/database.service';
import { EntityService } from './core/services/entity.service';
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
  private readonly onlineShareService = inject(OnlineShareService);
  private readonly workspaceService = inject(WorkspaceService);
  private readonly syncService = inject(SyncService);
  private readonly updateService = inject(UpdateService);

  readonly searchQuery = signal('');
  readonly activeNav = signal('inicio');
  readonly universes = signal<UniverseWithStats[]>([]);
  readonly stories = signal<Story[]>([]);
  readonly books = signal<Book[]>([]);
  readonly chapters = signal<Chapter[]>([]);
  readonly universeChapters = signal<ChapterOption[]>([]);
  readonly entities = signal<Entity[]>([]);
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
  readonly shareLink = signal('');
  readonly shareExpiresAt = signal('');
  readonly onlineShares = signal<StoredOnlineShare[]>(this.readStoredShares());
  readonly updateBusy = signal(false);
  readonly updateProgress = signal(0);
  readonly updatePhase = signal<'idle' | 'checking' | 'available' | 'downloading' | 'current' | 'error'>('idle');
  readonly updateInfo = signal<AppUpdateInfo>({ currentVersion: '0.4.0', availableVersion: null, notes: '', publishedAt: null });
  readonly updateError = signal('');
  readonly updatePromptDismissed = signal(false);
  readonly syncStatus = signal<SyncServerStatus>({ running: false, address: null, pairing_code: null, device_name: 'Meu computador' });
  readonly lastOpenedUniverseId = signal<string | null>(localStorage.getItem('narrahub.lastUniverseId'));
  readonly pendingDeleteUniverse = signal<UniverseWithStats | null>(null);

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
    { id: 'personagens', label: 'Personagens', icon: '♙', needsUniverse: true },
    { id: 'lugares', label: 'Lugares', icon: '⌖', needsUniverse: true },
    { id: 'eventos', label: 'Eventos', icon: '◇', needsUniverse: true },
    { id: 'objetos', label: 'Objetos', icon: '△', needsUniverse: true },
    { id: 'organizacoes', label: 'Organizações', icon: '◆', needsUniverse: true },
    { id: 'conexoes', label: 'Conexões', icon: '⌘', needsUniverse: true },
    { id: 'timeline', label: 'Timeline', icon: '◷', needsUniverse: true },
    { id: 'planejamento', label: 'Planejamento', icon: '☑', needsUniverse: true },
    { id: 'historico', label: 'Histórico', icon: '↶', needsUniverse: true },
    { id: 'configuracoes', label: 'Configurações', icon: '⚙', needsUniverse: false },
  ];

  readonly wordCount = computed(() => this.countWords(this.editorContent()));

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
    else if (item.id === 'personagens') this.appState.openEntityList('Personagem');
    else if (item.id === 'lugares') this.appState.openEntityList('Lugar');
    else if (item.id === 'eventos') this.appState.openEntityList('Evento');
    else if (item.id === 'objetos') this.appState.openEntityList('Objeto');
    else if (item.id === 'organizacoes') this.appState.openEntityList('Organização');
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
      const [stories, entities, chapters] = await Promise.all([
        this.storyService.listByUniverse(id),
        this.entityService.listByUniverse(id),
        this.chapterService.listByUniverse(id),
      ]);
      if (epoch !== this.workspaceEpoch || this.appState.activeUniverseId() !== id) return;
      this.stories.set(stories); this.entities.set(entities); this.universeChapters.set(chapters);
      if (this.stories().length) await this.selectStory(this.stories()[0]); else this.clearWritingSelection();
    } catch (error) { this.reportError('Não foi possível carregar os dados do universo.', error); }
  }

  async createStory(): Promise<void> {
    const id = this.appState.activeUniverseId(); const name = this.newStoryName.trim(); if (!id || !name) return;
    try {
      const story = await this.storyService.create(id, name); this.newStoryName = ''; this.appState.closeModal();
      this.stories.set(await this.storyService.listByUniverse(id)); await this.selectStory(story); await this.refreshUniverseStats();
    } catch (error) { this.reportError('Não foi possível criar a história.', error); }
  }

  async selectStory(story: Story): Promise<void> {
    await this.saveChapterNow();
    const universeId = this.appState.activeUniverseId();
    const books = await this.bookService.listByStory(story.id);
    if (!universeId || this.appState.activeUniverseId() !== universeId || !this.stories().some((item) => item.id === story.id)) return;
    this.activeStory.set(story); this.appState.activeStoryId.set(story.id); this.books.set(books);
    if (this.books().length) await this.selectBook(this.books()[0]);
    else { this.activeBook.set(null); this.activeChapter.set(null); this.chapters.set([]); this.editorTitle.set(''); this.editorContent.set(''); }
  }

  async createBook(): Promise<void> {
    const story = this.activeStory(); const name = this.newBookName.trim(); if (!story || !name) return;
    try {
      const book = await this.bookService.create(story.id, name); this.newBookName = ''; this.appState.closeModal();
      this.books.set(await this.bookService.listByStory(story.id)); await this.selectBook(book); await this.refreshUniverseStats();
    } catch (error) { this.reportError('Não foi possível criar o livro.', error); }
  }

  async selectBook(book: Book): Promise<void> {
    await this.saveChapterNow();
    const storyId = this.activeStory()?.id;
    const chapters = await this.chapterService.listByBook(book.id);
    if (!storyId || this.activeStory()?.id !== storyId || !this.books().some((item) => item.id === book.id)) return;
    this.activeBook.set(book); this.appState.activeBookId.set(book.id); this.chapters.set(chapters);
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
      this.saveMessage.set('Salvo'); await this.refreshUniverseStats();
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
  saveShareServer(): void {
    try {
      if (!this.shareApiUrl.trim()) {
        localStorage.removeItem('narrahub.shareApiUrl'); this.shareApiUrl = ''; this.showInfo('Servidor de compartilhamento removido.'); return;
      }
      this.shareApiUrl = this.onlineShareService.normalizeApiUrl(this.shareApiUrl);
      localStorage.setItem('narrahub.shareApiUrl', this.shareApiUrl);
      this.showInfo('Servidor de compartilhamento salvo neste dispositivo.');
    } catch (error) { this.reportError('Endereço de compartilhamento inválido.', error); }
  }
  async createOnlineShare(): Promise<void> {
    const chapter = this.activeChapter(); if (!chapter) return;
    if (!this.shareApiUrl.trim()) { this.showInfo('Configure o servidor em Configurações > Compartilhamento online.'); return; }
    this.errorMessage.set('');
    this.shareBusy.set(true);
    try {
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
  private clearWritingSelection(): void { this.activeStory.set(null); this.activeBook.set(null); this.activeChapter.set(null); this.books.set([]); this.chapters.set([]); this.editorTitle.set(''); this.editorContent.set(''); }
  private resetWorkspaceData(): void { this.clearWritingSelection(); this.stories.set([]); this.universeChapters.set([]); this.entities.set([]); this.timeline.set([]); this.planning.set([]); this.relations.set([]); this.history.set([]); this.activeEntity.set(null); this.entityGallery.set([]); }
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
