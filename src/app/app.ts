import { CommonModule } from '@angular/common';
import { Component, HostListener, OnDestroy, OnInit, ViewChild, computed, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { CdkDragDrop, DragDropModule, moveItemInArray } from '@angular/cdk/drag-drop';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Book, BookOption, Chapter, ChapterOption, ContentTag, ContentTagAssignment, Entity, EntityWithDetails,
  MentionOccurrence, MetadataOwnerType, PlanningItem, RelationCard, Story, SyncServerStatus, UniverseWithStats,
} from './core/models';
import { BookService } from './core/services/book.service';
import { BackupManifest, BackupService, BackupValidation, DatabaseHealthReport, RestorePreparation } from './core/services/backup.service';
import { AiMode, AiModelProfile, AiService } from './core/services/ai.service';
import { ChapterService } from './core/services/chapter.service';
import { CollaborationContribution, CollaborationService, CollaborationSession, SharePermission } from './core/services/collaboration.service';
import { DatabaseService } from './core/services/database.service';
import { AppNavigationId, AppRouteState } from './core/navigation/app-navigation';
import { AppNavigationService } from './core/navigation/app-navigation.service';
import { MentionService } from './core/services/mention.service';
import { MetadataService } from './core/services/metadata.service';
import { OnlineShareDocument, OnlineShareService, OnlineShareStatus, SharedUniverse } from './core/services/online-share.service';
import { PlanningService } from './core/services/planning.service';
import { StoryService } from './core/services/story.service';
import { SyncService } from './core/services/sync.service';
import { ThemePreference, ThemeService } from './core/services/theme.service';
import { AppUpdateInfo, UpdateService } from './core/services/update.service';
import { WorkspaceService } from './core/services/workspace.service';
import { AppState } from './core/state/app.state';
import { fileToDataUrl } from './shared/utils/file-to-data-url';
import { ConnectionsGraphComponent } from './features/connections/connections-graph.component';
import { EntitiesPageComponent, EntityMutationKind } from './features/entities/entities-page/entities-page.component';
import { EntityHubType, EntityStore } from './features/entities/state/entity.store';
import { ProductionReplicaComponent } from './features/production-replica/production-replica.component';
import { PlanningBoardComponent } from './features/planning/planning-board.component';
import { HistoryPageComponent } from './features/history/history-page.component';
import { HistoryStore } from './features/history/state/history.store';
import { LibraryPageComponent } from './features/library/library-page.component';
import { UniverseStore } from './features/library/state/universe.store';
import { TimelinePageComponent } from './features/timeline/timeline-page.component';
import { TimelineStore } from './features/timeline/state/timeline.store';
import { AiWritingRequest, WritingEditorComponent } from './features/writing/writing-editor.component';
import { AppShellComponent } from './shell/app-shell/app-shell.component';
import { ContextualInspectorComponent } from './shell/contextual-inspector/contextual-inspector.component';
import { TitlebarComponent } from './shell/titlebar/titlebar.component';
import { SidebarNavItem, UniverseSidebarComponent } from './shell/universe-sidebar/universe-sidebar.component';

interface StoredOnlineShare {
  id: string;
  revokeToken: string;
  expiresAt: string;
  title: string;
  encryptionKey: string;
  permission: SharePermission;
  universeIds: string[];
  lastSequence: number;
}

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

type DeleteKind = 'story' | 'book' | 'chapter' | 'relation';
type RenameKind = 'universe' | 'story' | 'book' | 'chapter';

interface PendingDelete {
  kind: DeleteKind;
  id: string;
  name: string;
  detail: string;
}

interface PendingRename {
  kind: RenameKind;
  id: string;
  name: string;
}

interface MetadataTarget {
  type: MetadataOwnerType;
  id: string;
  name: string;
}

type SettingsSection = 'general' | 'ai' | 'sync' | 'share' | 'updates';

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
    LibraryPageComponent,
    EntitiesPageComponent,
    ConnectionsGraphComponent,
    ProductionReplicaComponent,
    PlanningBoardComponent,
    HistoryPageComponent,
    TimelinePageComponent,
    WritingEditorComponent,
  ],
  templateUrl: './app.html',
  styleUrl: './app.css',
})
export class App implements OnInit, OnDestroy {
  readonly Math = Math;
  readonly appState = inject(AppState);
  readonly theme = inject(ThemeService);
  readonly ai = inject(AiService);
  private readonly db = inject(DatabaseService);
  private readonly universeStore = inject(UniverseStore);
  private readonly storyService = inject(StoryService);
  private readonly bookService = inject(BookService);
  private readonly backupService = inject(BackupService);
  private readonly chapterService = inject(ChapterService);
  private readonly collaborationService = inject(CollaborationService);
  private readonly entityStore = inject(EntityStore);
  private readonly mentionService = inject(MentionService);
  private readonly metadataService = inject(MetadataService);
  private readonly onlineShareService = inject(OnlineShareService);
  private readonly planningService = inject(PlanningService);
  private readonly workspaceService = inject(WorkspaceService);
  private readonly syncService = inject(SyncService);
  private readonly updateService = inject(UpdateService);
  private readonly historyStore = inject(HistoryStore);
  private readonly timelineStore = inject(TimelineStore);
  private readonly navigation = inject(AppNavigationService);

  readonly searchQuery = signal('');
  readonly activeNav = signal('inicio');
  readonly universes = this.universeStore.universes;
  readonly stories = signal<Story[]>([]);
  readonly books = signal<Book[]>([]);
  readonly universeBooks = signal<BookOption[]>([]);
  readonly chapters = signal<Chapter[]>([]);
  readonly universeChapters = signal<ChapterOption[]>([]);
  readonly entities = this.entityStore.entities;
  readonly entityFilter = this.entityStore.filter;
  readonly mentionOccurrences = signal<MentionOccurrence[]>([]);
  readonly timeline = this.timelineStore.events;
  readonly planning = signal<PlanningItem[]>([]);
  readonly relations = signal<RelationCard[]>([]);
  readonly activeStory = signal<Story | null>(null);
  readonly activeBook = signal<Book | null>(null);
  readonly activeChapter = signal<Chapter | null>(null);
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
  readonly shareProgressMessage = signal('');
  readonly shareSession = signal<OnlineShareStatus>({ running: false, publicUrl: null, shareCount: 0 });
  readonly shareLink = signal('');
  readonly shareExpiresAt = signal('');
  readonly onlineShares = signal<StoredOnlineShare[]>([]);
  readonly shareSelectedUniverseIds = signal<Set<string>>(new Set());
  readonly shareIncludeChapters = signal(true);
  readonly shareIncludeEntities = signal(true);
  readonly collaborationSessions = signal<CollaborationSession[]>([]);
  readonly collaborationContributions = signal<CollaborationContribution[]>([]);
  readonly selectedCollaborationSessionId = signal<string | null>(null);
  readonly updateBusy = signal(false);
  readonly updateProgress = signal(0);
  readonly updatePhase = signal<'idle' | 'checking' | 'available' | 'backing-up' | 'downloading' | 'current' | 'error'>('idle');
  readonly updateInfo = signal<AppUpdateInfo>({ currentVersion: '0.7.4', availableVersion: null, notes: '', publishedAt: null });
  readonly updateError = signal('');
  readonly updatePromptDismissed = signal(false);
  readonly backupBusy = signal(false);
  readonly backupError = signal('');
  readonly databaseHealth = signal<DatabaseHealthReport | null>(null);
  readonly backups = signal<BackupManifest[]>([]);
  readonly lastBackupValidation = signal<BackupValidation | null>(null);
  readonly pendingRestoreBackup = signal<BackupManifest | null>(null);
  readonly restorePreparation = signal<RestorePreparation | null>(null);
  readonly syncStatus = signal<SyncServerStatus>({ running: false, address: null, pairing_code: null, device_name: 'Meu computador' });
  readonly lastOpenedUniverseId = signal<string | null>(localStorage.getItem('narrahub.lastUniverseId'));
  readonly pendingDelete = signal<PendingDelete | null>(null);
  readonly pendingRename = signal<PendingRename | null>(null);
  readonly aiBusy = signal(false);
  readonly aiResponse = signal('');
  readonly aiError = signal('');
  readonly aiInstallBusy = signal(false);
  readonly aiInstallError = signal('');
  readonly aiWritingRequest = signal<AiWritingRequest | null>(null);
  readonly chapterSummary = signal('');
  readonly inspectorOpen = signal(localStorage.getItem('narrahub.inspectorOpen') !== 'false');
  readonly metadataTarget = signal<MetadataTarget | null>(null);
  readonly metadataTags = signal<ContentTag[]>([]);
  readonly metadataOwnerTags = signal<ContentTag[]>([]);
  readonly libraryPreviewTags = signal<Record<string, ContentTag[]>>({});
  readonly workspacePreviewTags = signal<Record<string, ContentTag[]>>({});
  readonly chapterTags = signal<ContentTag[]>([]);
  readonly settingsSection = signal<SettingsSection>('general');
  readonly expandedStoryIds = signal<Set<string>>(new Set());
  readonly expandedBookIds = signal<Set<string>>(new Set());

  newStoryName = '';
  newBookName = '';
  newChapterTitle = '';
  newRelationSource = '';
  newRelationTarget = '';
  newRelationLabel = '';
  aiMode: AiMode = this.ai.settings().mode;
  aiEndpoint = this.ai.settings().endpoint;
  aiModel = this.ai.settings().model;
  aiApiKey = this.ai.sessionApiKey;
  aiWriterGuidance = this.ai.writerGuidance();
  aiSelectedProfile: AiModelProfile['id'] = this.ai.localStatus().recommended.id;
  aiPrompt = '';
  newTagName = '';
  newTagColor = '#7d3650';
  renameValue = '';
  restoreConfirmation = '';
  deviceName = localStorage.getItem('narrahub.deviceName') || 'Meu computador';
  remoteAddress = '';
  pairingCode = '';
  shareExpiresInDays = 7;
  sharePermission: SharePermission = 'view';

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
  readonly shareSelectionCount = computed(() => this.shareSelectedUniverseIds().size);
  readonly pendingCollaborationCount = computed(() => this.collaborationContributions().filter((item) => item.status === 'pending').length);
  readonly selectedCollaborationContributions = computed(() => {
    const id = this.selectedCollaborationSessionId();
    return id ? this.collaborationContributions().filter((item) => item.session_id === id) : [];
  });
  readonly selectedCollaborationHasPending = computed(() => this.selectedCollaborationContributions().some((item) => item.status === 'pending'));
  readonly globalSearchResults = computed<GlobalSearchResult[]>(() => {
    if (this.appState.currentView() !== 'workspace') return [];
    const query = this.normalizeSearch(this.searchQuery());
    if (query.length < 2) return [];
    const matches = (value: string) => this.normalizeSearch(value).includes(query);
    const results: GlobalSearchResult[] = [];
    for (const story of this.stories()) if (matches(`${story.name} ${story.description}`)) results.push({ id: story.id, kind: 'story', label: story.name, context: 'História', icon: '⌂' });
    for (const book of this.universeBooks()) if (matches(`${book.name} ${book.description} ${book.story_name}`)) results.push({ id: book.id, kind: 'book', label: book.name, context: `Livro · ${book.story_name}`, icon: '▱' });
    for (const chapter of this.universeChapters()) if (matches(`${chapter.title} ${chapter.content} ${chapter.book_name} ${chapter.story_name}`)) results.push({ id: chapter.id, kind: 'chapter', label: chapter.title, context: `${chapter.story_name} · ${chapter.book_name}`, icon: '▤' });
    for (const entity of this.entities()) if (matches(`${entity.name} ${entity.summary} ${entity.description} ${entity.type}`)) results.push({ id: entity.id, kind: 'entity', label: entity.name, context: entity.type, icon: entity.name.charAt(0).toUpperCase() });
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
  readonly characterEntities = computed(() => this.entities()
    .filter((entity) => entity.type === 'Personagem')
    .map(({ id, name, image }) => ({ id, name, image })));
  readonly writingPlaces = computed(() => {
    const paragraphs = this.contentParagraphs(this.editorContent());
    return this.entities().filter((entity) => entity.type === 'Lugar' && paragraphs.some((paragraph) => this.textMentionsEntity(paragraph, entity.name)));
  });

  @ViewChild(WritingEditorComponent) private writingEditor?: WritingEditorComponent;
  @ViewChild(PlanningBoardComponent) private planningBoard?: PlanningBoardComponent;
  @ViewChild(TimelinePageComponent) private timelinePage?: TimelinePageComponent;
  @ViewChild(EntitiesPageComponent) readonly entitiesPage?: EntitiesPageComponent;

  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private infoTimer: ReturnType<typeof setTimeout> | null = null;
  private collaborationTimer: ReturnType<typeof setInterval> | null = null;
  private workspaceEpoch = 0;
  private restoringRoute = false;

  constructor() {
    effect(() => {
      const route = this.navigation.route();
      if (!this.isLoading()) void this.restoreRoute(route);
    });
  }

  async ngOnInit(): Promise<void> {
    await this.ai.initialize().catch((error) => {
      console.error('[NarraHub] Não foi possível inicializar o gerenciador da IA local.', error);
      this.aiInstallError.set(error instanceof Error ? error.message : String(error));
    });
    this.aiSelectedProfile = this.ai.localStatus().installedProfile as AiModelProfile['id'] || this.ai.localStatus().recommended.id;
    if (!isTauri()) { this.isLoading.set(false); return; }
    try {
      await this.db.init();
      await this.loadUniverses();
      this.syncStatus.set(await this.syncService.status());
      this.shareSession.set(await this.onlineShareService.status());
      await this.loadCollaborationReview();
      this.collaborationTimer = setInterval(() => void this.syncCollaborationContributions(), 2500);
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
    if (this.collaborationTimer) clearInterval(this.collaborationTimer);
    this.updateService.dispose();
    this.ai.dispose();
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
  async closeWindow(): Promise<void> {
    await this.saveChapterNow();
    await this.syncCollaborationContributions();
    await this.collaborationService.endAllActive('ended').catch(() => undefined);
    await this.onlineShareService.stop().catch((error) => console.warn('[NarraHub] Falha ao encerrar compartilhamento temporário.', error));
    if (isTauri()) await getCurrentWindow().close();
  }
  async toggleFullscreen(): Promise<void> { if (isTauri()) { const win = getCurrentWindow(); await win.setFullscreen(!(await win.isFullscreen())); } }
  async loadUniverses(): Promise<void> {
    await this.universeStore.load();
    await this.refreshLibraryPreviewTags();
  }

  async selectNav(item: SidebarNavItem, updateRoute = true): Promise<void> {
    if (item.needsUniverse && !this.appState.activeUniverse()) {
      this.showInfo('Selecione ou crie um universo para abrir esta área.'); return;
    }
    await this.saveChapterNow();
    this.activeNav.set(item.id);
    if (item.id === 'inicio') { await this.returnToLibrary(updateRoute); return; }
    if (item.id === 'ajuda') { this.showInfo('Ajuda e feedback serão conectados ao fluxo nativo em uma próxima fase.'); return; }
    if (item.id === 'configuracoes') { this.openSettings(updateRoute); return; }
    if (item.id === 'escrita') this.appState.openEditor();
    else if (item.id === 'entidades') {
      this.entityStore.setFilter(null);
      this.appState.openEntityList();
    }
    else if (item.id === 'conexoes') { this.appState.openGraph(); await this.loadRelations(); }
    else if (item.id === 'timeline') this.appState.openTimeline();
    else if (item.id === 'planejamento') { this.appState.openPlanning(); await this.loadPlanning(); }
    else if (item.id === 'historico') this.appState.openHistory();
    if (updateRoute) await this.navigation.navigate(item.id as AppNavigationId, this.appState.activeUniverseId());
  }

  async openUniverse(universe: UniverseWithStats, updateRoute = true): Promise<void> {
    await this.saveChapterNow();
    this.workspaceEpoch += 1; this.resetWorkspaceData();
    localStorage.setItem('narrahub.lastUniverseId', universe.id); this.lastOpenedUniverseId.set(universe.id);
    this.searchQuery.set(''); this.appState.openUniverse(universe); this.activeNav.set('escrita'); await this.loadWorkspaceData();
    if (updateRoute) await this.navigation.navigate('escrita', universe.id);
  }

  onUniverseUpdated(): void { this.showInfo('Universo atualizado.'); }

  onUniverseDeleted(universeId: string): void {
    if (this.lastOpenedUniverseId() === universeId) { localStorage.removeItem('narrahub.lastUniverseId'); this.lastOpenedUniverseId.set(null); }
    if (this.appState.activeUniverseId() === universeId) void this.returnToLibrary();
    this.showInfo('Universo excluído do banco local.');
  }

  requestDelete(kind: DeleteKind, id: string, name: string, event?: Event): void {
    event?.stopPropagation();
    const detail: Record<DeleteKind, string> = {
      story: 'Os livros e capítulos desta história também serão excluídos.',
      book: 'Os capítulos deste livro também serão excluídos.',
      chapter: 'O texto, as revisões e as menções deste capítulo serão excluídos.',
      relation: 'A ligação será removida. As entidades conectadas serão preservadas.',
    };
    this.pendingDelete.set({ kind, id, name, detail: detail[kind] });
    this.appState.openModal('delete-item');
  }

  async confirmDeleteItem(): Promise<void> {
    const pending = this.pendingDelete();
    if (!pending) return;
    try {
      await this.saveChapterNow();
      if (pending.kind === 'story') await this.deleteStoryRecord(pending.id);
      else if (pending.kind === 'book') await this.deleteBookRecord(pending.id);
      else if (pending.kind === 'chapter') await this.deleteChapterRecord(pending.id);
      else await this.workspaceService.deleteRelation(pending.id);

      this.pendingDelete.set(null);
      this.appState.closeModal();
      await Promise.all([this.refreshUniverseStats(), this.refreshUniverseBooks(), this.refreshUniverseChapters()]);
      if (pending.kind === 'relation') await this.loadRelations();
      this.showInfo(`${this.deleteKindLabel(pending.kind)} excluído(a) do banco local.`);
    } catch (error) { this.reportError(`Não foi possível excluir ${pending.name}.`, error); }
  }

  deleteKindLabel(kind: DeleteKind): string {
    return ({ story: 'História', book: 'Livro', chapter: 'Capítulo', relation: 'Ligação' } as Record<DeleteKind, string>)[kind];
  }

  requestRename(kind: RenameKind, id: string, name: string, event?: Event): void {
    event?.stopPropagation();
    this.pendingRename.set({ kind, id, name });
    this.renameValue = name;
    this.appState.openModal('rename-item');
  }

  renameKindLabel(kind: RenameKind): string {
    return ({
      universe: 'Universo', story: 'História', book: 'Livro', chapter: 'Capítulo',
    } as Record<RenameKind, string>)[kind];
  }

  async confirmRename(): Promise<void> {
    const pending = this.pendingRename();
    const name = this.renameValue.trim();
    if (!pending || !name) return;
    try {
      await this.saveChapterNow();
      if (pending.kind === 'universe') {
        this.appState.activeUniverse.update((item) => item?.id === pending.id ? { ...item, name, updated_at: this.db.now() } : item);
        await this.universeStore.update(pending.id, { name });
      } else if (pending.kind === 'story') {
        await this.storyService.update(pending.id, { name });
        this.stories.update((items) => items.map((item) => item.id === pending.id ? { ...item, name } : item));
        this.activeStory.update((item) => item?.id === pending.id ? { ...item, name } : item);
        this.universeBooks.update((items) => items.map((item) => item.story_id === pending.id ? { ...item, story_name: name } : item));
        this.universeChapters.update((items) => items.map((item) => item.story_id === pending.id ? { ...item, story_name: name } : item));
        await this.loadPlanning();
      } else if (pending.kind === 'book') {
        await this.bookService.update(pending.id, { name });
        this.books.update((items) => items.map((item) => item.id === pending.id ? { ...item, name } : item));
        this.activeBook.update((item) => item?.id === pending.id ? { ...item, name } : item);
        this.universeBooks.update((items) => items.map((item) => item.id === pending.id ? { ...item, name } : item));
        this.universeChapters.update((items) => items.map((item) => item.book_id === pending.id ? { ...item, book_name: name } : item));
        await this.loadPlanning();
      } else if (pending.kind === 'chapter') {
        await this.chapterService.updateTitle(pending.id, name);
        this.chapters.update((items) => items.map((item) => item.id === pending.id ? { ...item, title: name } : item));
        this.universeChapters.update((items) => items.map((item) => item.id === pending.id ? { ...item, title: name } : item));
        this.activeChapter.update((item) => item?.id === pending.id ? { ...item, title: name } : item);
        if (this.activeChapter()?.id === pending.id) { this.editorTitle.set(name); this.saveMessage.set('Salvo'); }
        await this.loadPlanning();
      }
      this.pendingRename.set(null); this.renameValue = ''; this.appState.closeModal();
      this.showInfo(`${this.renameKindLabel(pending.kind)} renomeado(a).`);
    } catch (error) { this.reportError(`Não foi possível renomear ${pending.name}.`, error); }
  }

  async returnToLibrary(updateRoute = true): Promise<void> {
    await this.saveChapterNow(); this.workspaceEpoch += 1; this.appState.goHome(); this.activeNav.set('inicio'); this.searchQuery.set(''); this.resetWorkspaceData();
    if (updateRoute) await this.navigation.navigate('inicio', null);
  }

  openSettings(updateRoute = true): void { this.searchQuery.set(''); this.activeNav.set('configuracoes'); this.appState.openSettings(); void this.refreshBackupStatus(); if (updateRoute) void this.navigation.navigate('configuracoes', null); }

  openShareModal(): void {
    if (!this.appState.activeUniverse()) { this.showInfo('Abra um universo antes de compartilhar.'); return; }
    this.shareSelectedUniverseIds.set(new Set([this.appState.activeUniverse()!.id]));
    this.shareIncludeChapters.set(true);
    this.shareIncludeEntities.set(true);
    this.sharePermission = 'view';
    this.shareLink.set(''); this.shareExpiresAt.set(''); this.shareProgressMessage.set(''); this.appState.openModal('share-content');
  }

  async loadWorkspaceData(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const epoch = this.workspaceEpoch;
    try {
      const [stories, books, , chapters, , planning, tagAssignments] = await Promise.all([
        this.storyService.listByUniverse(id),
        this.bookService.listByUniverse(id),
        this.entityStore.load(id, true),
        this.chapterService.listByUniverse(id),
        this.timelineStore.load(id),
        this.planningService.list(id),
        this.metadataService.listAssignments([id]),
      ]);
      if (epoch !== this.workspaceEpoch || this.appState.activeUniverseId() !== id) return;
      const entities = this.entityStore.entities();
      this.stories.set(stories); this.universeBooks.set(books); this.universeChapters.set(chapters); this.planning.set(planning); this.workspacePreviewTags.set(this.groupTagAssignments(tagAssignments));
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
    else { this.activeBook.set(null); this.activeChapter.set(null); this.chapters.set([]); this.editorTitle.set(''); this.editorContent.set(''); this.chapterSummary.set(''); }
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
    if (this.chapters().length) await this.selectChapter(this.chapters()[0]); else { this.activeChapter.set(null); this.editorTitle.set(''); this.editorContent.set(''); this.chapterSummary.set(''); }
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
    this.editorTitle.set(chapter.title); this.editorContent.set(chapter.content || ''); this.chapterSummary.set(chapter.summary || ''); this.saveMessage.set('Salvo');
    await this.loadChapterMetadata(chapter.id);
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
    this.setWorkspaceNavigation('escrita'); this.appState.openEditor();
  }

  async openChapterOption(option: ChapterOption): Promise<void> {
    const story = this.stories().find((candidate) => candidate.id === option.story_id); if (!story) return;
    await this.selectStory(story);
    const book = this.books().find((candidate) => candidate.id === option.book_id); if (!book) return;
    await this.selectBook(book);
    const chapter = this.chapters().find((candidate) => candidate.id === option.id); if (chapter) await this.selectChapter(chapter);
    this.setWorkspaceNavigation('escrita'); this.appState.openEditor(option.id);
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
    const cover_image = await fileToDataUrl(file); await this.bookService.update(book.id, { cover_image });
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
  onChapterSummaryInput(summary: string): void { this.chapterSummary.set(summary); this.queueChapterSave(); }
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
    const title = this.editorTitle().trim() || 'Capítulo sem título'; const content = this.editorContent(); const summary = this.chapterSummary().trim(); const words = this.countWords(content); this.isSaving.set(true);
    try {
      if (title !== chapter.title) await this.chapterService.updateTitle(chapter.id, title);
      if (content !== chapter.content || words !== chapter.word_count) await this.chapterService.updateContent(chapter.id, content, words);
      if (summary !== (chapter.summary || '')) await this.chapterService.updateSummary(chapter.id, summary);
      const updated = { ...chapter, title, content, summary, word_count: words, updated_at: this.db.now() };
      this.activeChapter.set(updated); this.chapters.update((items) => items.map((item) => item.id === updated.id ? updated : item));
      this.universeChapters.update((items) => items.map((item) => item.id === updated.id ? { ...item, ...updated } : item));
      await this.mentionService.syncChapterMentions(chapter.id, this.entityIdsInContent(content, this.entities()));
      this.saveMessage.set('Salvo'); await Promise.all([this.refreshUniverseStats(), this.refreshMentionOccurrences()]);
    } catch (error) { this.saveMessage.set('Erro ao salvar'); this.reportError('O capítulo não foi salvo.', error); }
    finally { this.isSaving.set(false); }
  }

  beginCreateEntity(): void { this.entitiesPage?.openCreate(); }

  selectEntityTab(type: EntityHubType | null): void {
    this.entityStore.setFilter(type);
    this.appState.openEntityList();
  }

  entityCreateLabel(): string { return this.entityStore.createLabel(); }

  async openEntitySheet(entity: Entity): Promise<void> {
    const universeId = this.appState.activeUniverseId();
    if (!universeId || !await this.entityStore.open(universeId, entity)) {
      this.reportError('Não foi possível abrir a ficha.', this.entityStore.error());
      return;
    }
    this.appState.openEntitySheet();
  }

  onEntityViewChanged(view: 'entities' | 'entity-sheet'): void {
    if (view === 'entities') this.appState.openEntityList();
    else if (this.entityStore.activeEntity()) this.appState.openEntitySheet();
  }

  async onEntityMutation(kind: EntityMutationKind): Promise<void> {
    if (kind === 'created') await this.refreshUniverseStats();
    else if (kind === 'renamed') await Promise.all([this.loadRelations(), this.refreshMentionOccurrences()]);
    else if (kind === 'deleted') {
      await Promise.all([
        this.refreshUniverseStats(),
        this.refreshUniverseBooks(),
        this.refreshUniverseChapters(),
        this.loadRelations(),
        this.refreshMentionOccurrences(),
      ]);
    }
  }

  onEntityInfo(message: string): void { this.showInfo(message); }

  onEntityFailure(message: string): void { this.reportError(message, message); }

  async openGlobalSearchResult(result: GlobalSearchResult): Promise<void> {
    this.searchQuery.set('');
    if (result.kind === 'story') {
      const story = this.stories().find((item) => item.id === result.id); if (story) { await this.selectStory(story); this.setWorkspaceNavigation('escrita'); this.appState.openEditor(); }
    } else if (result.kind === 'book') {
      const book = this.universeBooks().find((item) => item.id === result.id); if (book) await this.openBookOption(book);
    } else if (result.kind === 'chapter') {
      const chapter = this.universeChapters().find((item) => item.id === result.id); if (chapter) await this.openChapterOption(chapter);
    } else if (result.kind === 'entity') {
      const entity = this.entities().find((item) => item.id === result.id); if (entity) { this.setWorkspaceNavigation('entidades'); this.selectEntityTab(entity.type as EntityHubType); await this.openEntitySheet(entity); }
    } else if (result.kind === 'timeline') {
      this.setWorkspaceNavigation('timeline'); this.appState.openTimeline(); queueMicrotask(() => document.querySelector<HTMLElement>(`[data-timeline-id="${result.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' }));
    } else {
      this.setWorkspaceNavigation('planejamento'); this.appState.openPlanning(); queueMicrotask(() => document.querySelector<HTMLElement>(`[data-planning-id="${result.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' }));
    }
  }

  async loadRelations(): Promise<void> { const id = this.appState.activeUniverseId(); if (!id) return; const data = await this.workspaceService.listRelations(id); if (this.appState.activeUniverseId() === id) this.relations.set(data); }
  async createRelation(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id || !this.newRelationSource || !this.newRelationTarget || !this.newRelationLabel.trim()) return;
    if (this.newRelationSource === this.newRelationTarget) { this.showInfo('Escolha duas entidades diferentes.'); return; }
    await this.workspaceService.createRelation(id, this.newRelationSource, this.newRelationTarget, this.newRelationLabel.trim());
    this.newRelationSource = ''; this.newRelationTarget = ''; this.newRelationLabel = ''; this.appState.closeModal(); await this.loadRelations();
  }
  async deleteRelation(id: string): Promise<void> { await this.workspaceService.deleteRelation(id); await this.loadRelations(); }

  async loadPlanning(): Promise<void> { const id = this.appState.activeUniverseId(); if (!id) return; const data = await this.planningService.list(id); if (this.appState.activeUniverseId() === id) this.planning.set(data); }
  beginCreatePlanning(): void { this.planningBoard?.openCreate(); }
  beginCreateTimeline(): void { this.timelinePage?.openCreate(); }

  async openPlanningChapter(item: PlanningItem): Promise<void> {
    const option = this.universeChapters().find((chapter) => chapter.id === item.chapter_id); if (!option) return;
    const story = this.stories().find((candidate) => candidate.id === option.story_id); if (!story) return;
    await this.selectStory(story);
    const book = this.books().find((candidate) => candidate.id === option.book_id); if (!book) return;
    await this.selectBook(book);
    const chapter = this.chapters().find((candidate) => candidate.id === option.id); if (chapter) await this.selectChapter(chapter);
    this.setWorkspaceNavigation('escrita'); this.appState.openEditor(option.id);
  }

  toggleInspector(): void {
    this.inspectorOpen.update((open) => !open);
    localStorage.setItem('narrahub.inspectorOpen', String(this.inspectorOpen()));
  }

  chapterField(key: string): string {
    const chapter = this.activeChapter();
    if (!chapter) return '';
    return key === 'Origem da cena' ? chapter.scene_origin : key === 'Destino da cena' ? chapter.scene_destination : '';
  }

  async updateChapterContext(key: string, value: string): Promise<void> {
    const chapter = this.activeChapter(); if (!chapter) return;
    const sceneOrigin = key === 'Origem da cena' ? value.trim() : chapter.scene_origin;
    const sceneDestination = key === 'Destino da cena' ? value.trim() : chapter.scene_destination;
    await this.chapterService.updateSceneRoute(chapter.id, sceneOrigin, sceneDestination);
    const updated = { ...chapter, scene_origin: sceneOrigin, scene_destination: sceneDestination };
    this.activeChapter.set(updated);
    this.chapters.update((items) => items.map((item) => item.id === chapter.id ? updated : item));
    this.universeChapters.update((items) => items.map((item) => item.id === chapter.id ? { ...item, ...updated } : item));
  }

  async openMetadata(type: MetadataOwnerType, id: string, name: string, event?: Event): Promise<void> {
    event?.stopPropagation();
    const universeId = type === 'universe' ? id : this.appState.activeUniverseId(); if (!universeId) return;
    this.metadataTarget.set({ type, id, name }); this.newTagName = '';
    const [tags, ownerTags] = await Promise.all([
      this.metadataService.listTags(universeId), this.metadataService.listOwnerTags(type, id),
    ]);
    this.metadataTags.set(tags); this.metadataOwnerTags.set(ownerTags); this.appState.openModal('metadata');
  }

  isMetadataTagAssigned(id: string): boolean { return this.metadataOwnerTags().some((tag) => tag.id === id); }

  previewTags(type: MetadataOwnerType, id: string): ContentTag[] {
    const key = this.tagPreviewKey(type, id);
    return this.workspacePreviewTags()[key] ?? this.libraryPreviewTags()[key] ?? [];
  }

  async createMetadataTag(): Promise<void> {
    const target = this.metadataTarget(); const universeId = target?.type === 'universe' ? target.id : this.appState.activeUniverseId(); const name = this.newTagName.trim();
    if (!universeId || !target || !name) return;
    try {
      const tag = await this.metadataService.createTag(universeId, name, this.newTagColor);
      await this.metadataService.setTag(target.type, target.id, tag.id, true); this.newTagName = '';
      await this.reloadMetadataTarget();
    } catch (error) { this.reportError('Não foi possível criar a tag. Verifique se esse nome já existe.', error); }
  }

  async toggleMetadataTag(tag: ContentTag): Promise<void> {
    const target = this.metadataTarget(); if (!target) return;
    await this.metadataService.setTag(target.type, target.id, tag.id, !this.isMetadataTagAssigned(tag.id)); await this.reloadMetadataTarget();
  }

  async deleteMetadataTag(tag: ContentTag): Promise<void> {
    if (!window.confirm(`Excluir a tag “${tag.name}” de todo o universo?`)) return;
    await this.metadataService.deleteTag(tag.id);
    await this.reloadMetadataTarget();
  }

  async summarizeChapterWithAi(): Promise<void> {
    const chapter = this.activeChapter(); if (!chapter || this.aiBusy()) return;
    if (!this.ai.enabled()) { this.showInfo('Ative a IA nas preferências para gerar o resumo.'); return; }
    this.aiBusy.set(true);
    try {
      const text = this.contentParagraphs(this.editorContent()).join('\n').slice(-16_000);
      const summary = await this.ai.complete(
        'Resuma este capítulo em um parágrafo objetivo. Inclua acontecimentos, mudança emocional e gancho final. Não invente fatos.',
        this.buildUniverseAiContext(`CAPÍTULO: ${this.editorTitle()}\n\nTEXTO:\n${text}`),
      );
      this.chapterSummary.set(summary); await this.chapterService.updateSummary(chapter.id, summary);
      const updated = { ...chapter, summary, updated_at: this.db.now() }; this.activeChapter.set(updated);
      this.chapters.update((items) => items.map((item) => item.id === chapter.id ? updated : item));
      this.universeChapters.update((items) => items.map((item) => item.id === chapter.id ? { ...item, summary } : item));
      this.saveMessage.set('Salvo');
    } catch (error) { this.reportError('A IA não conseguiu resumir o capítulo.', error); }
    finally { this.aiBusy.set(false); }
  }

  setTheme(value: string): void { this.theme.setTheme(value as ThemePreference); }

  setAiMode(mode: AiMode): void {
    this.aiMode = mode;
    this.aiInstallError.set('');
    if (mode === 'off') {
      this.ai.disable();
      this.showInfo('Assistência por IA desativada.');
    } else if (mode === 'local') {
      this.aiEndpoint = '';
      this.aiModel = '';
      this.aiSelectedProfile = this.ai.localStatus().installedProfile as AiModelProfile['id'] || this.ai.localStatus().recommended.id;
    }
  }

  async installLocalAi(profile: AiModelProfile['id']): Promise<void> {
    if (this.aiInstallBusy()) return;
    this.aiInstallBusy.set(true);
    this.aiInstallError.set('');
    try {
      await this.ai.installLocal(profile);
      this.aiMode = 'local';
      this.aiSelectedProfile = profile;
      this.showInfo('IA local instalada e iniciada. Ela será carregada automaticamente com o NarraHub.');
    } catch (error) {
      console.error('[NarraHub] A instalação da IA local falhou.', error);
      this.aiInstallError.set(error instanceof Error ? error.message : String(error));
    } finally { this.aiInstallBusy.set(false); }
  }

  async activateLocalAi(): Promise<void> {
    if (this.aiInstallBusy()) return;
    this.aiInstallBusy.set(true);
    this.aiInstallError.set('');
    try {
      this.ai.configure({ mode: 'local', endpoint: '', model: '' }, '');
      await this.ai.startLocalEngine(this.ai.localStatus().state === 'error');
      this.aiMode = 'local';
      this.showInfo('IA local iniciada.');
    } catch (error) { this.aiInstallError.set(error instanceof Error ? error.message : String(error)); }
    finally { this.aiInstallBusy.set(false); }
  }

  async restartLocalAi(): Promise<void> {
    if (this.aiInstallBusy()) return;
    this.aiInstallBusy.set(true);
    this.aiInstallError.set('');
    try { await this.ai.startLocalEngine(true); this.showInfo('IA local reiniciada em modo seguro.'); }
    catch (error) { this.aiInstallError.set(error instanceof Error ? error.message : String(error)); }
    finally { this.aiInstallBusy.set(false); }
  }

  formatAiSize(bytes: number): string {
    return bytes >= 1_000_000_000 ? `${(bytes / 1_000_000_000).toFixed(1)} GB` : `${Math.round(bytes / 1_000_000)} MB`;
  }

  selectedAiProfile(): AiModelProfile {
    return this.ai.localStatus().profiles.find((profile) => profile.id === this.aiSelectedProfile) || this.ai.localStatus().recommended;
  }

  installedAiProfile(): AiModelProfile {
    return this.ai.localStatus().profiles.find((profile) => profile.id === this.ai.localStatus().installedProfile) || this.ai.localStatus().recommended;
  }

  selectSettingsSection(section: SettingsSection): void { this.settingsSection.set(section); if (section === 'general') void this.refreshBackupStatus(); }

  async refreshBackupStatus(): Promise<void> {
    if (!isTauri() || this.backupBusy()) return;
    this.backupBusy.set(true); this.backupError.set('');
    try {
      const [health, backups] = await Promise.all([this.backupService.health(), this.backupService.list()]);
      this.databaseHealth.set(health); this.backups.set(backups);
    } catch (error) {
      this.backupError.set(error instanceof Error ? error.message : String(error));
    } finally { this.backupBusy.set(false); }
  }

  async createManualBackup(): Promise<void> {
    if (this.backupBusy()) return;
    await this.saveChapterNow();
    this.backupBusy.set(true); this.backupError.set(''); this.lastBackupValidation.set(null);
    try {
      const manifest = await this.backupService.create('manual');
      const validation = await this.backupService.validate(manifest.backupId);
      this.lastBackupValidation.set(validation);
      this.backups.set(await this.backupService.list());
      this.databaseHealth.set(validation.databaseHealth);
      if (!validation.valid) throw new Error(validation.errors.join(' '));
      this.showInfo('Backup local criado e validado.');
    } catch (error) {
      this.backupError.set(error instanceof Error ? error.message : String(error));
      this.reportError('Não foi possível criar um backup válido.', error);
    } finally { this.backupBusy.set(false); }
  }

  async validateBackup(backupId: string): Promise<void> {
    if (this.backupBusy()) return;
    this.backupBusy.set(true); this.backupError.set('');
    try {
      const validation = await this.backupService.validate(backupId);
      this.lastBackupValidation.set(validation);
      if (validation.valid) this.showInfo('Backup íntegro e compatível com o manifesto.');
      else this.backupError.set(validation.errors.join(' '));
    } catch (error) {
      this.backupError.set(error instanceof Error ? error.message : String(error));
    } finally { this.backupBusy.set(false); }
  }



  requestRestoreBackup(backup: BackupManifest): void {
    this.pendingRestoreBackup.set(backup);
    this.restorePreparation.set(null);
    this.restoreConfirmation = '';
    this.backupError.set('');
    this.appState.openModal('restore-backup');
  }

  async prepareRestoreBackup(): Promise<void> {
    const backup = this.pendingRestoreBackup();
    if (!backup || this.backupBusy()) return;
    if (this.shareSession().running || this.syncStatus().running) {
      this.backupError.set('Encerre o compartilhamento e a sincronização antes de restaurar um backup.');
      return;
    }
    await this.saveChapterNow();
    if (this.saveMessage() === 'Erro ao salvar') {
      this.backupError.set('A restauração foi interrompida porque o capítulo atual não pôde ser salvo.');
      return;
    }
    this.backupBusy.set(true);
    this.backupError.set('');
    try {
      const preparation = await this.backupService.prepareRestore(backup.backupId);
      this.restorePreparation.set(preparation);
      this.backups.set(await this.backupService.list());
    } catch (error) {
      this.backupError.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.backupBusy.set(false);
    }
  }

  async confirmRestoreBackup(): Promise<void> {
    const preparation = this.restorePreparation();
    if (!preparation || this.restoreConfirmation.trim() !== 'RESTAURAR' || this.backupBusy()) return;
    this.backupBusy.set(true);
    this.backupError.set('');
    try {
      await this.db.close();
      await this.backupService.commitRestore(preparation.token);
      await this.updateService.relaunch();
    } catch (error) {
      await this.db.init().catch((reopenError) => console.error('[NarraHub] Database reopen failed after restore error.', reopenError));
      this.backupError.set(error instanceof Error ? error.message : String(error));
      this.reportError('Não foi possível concluir a restauração recuperável.', error);
    } finally {
      this.backupBusy.set(false);
    }
  }

  backupReasonLabel(reason: BackupManifest['reason']): string {
    if (reason === 'manual') return 'Manual';
    if (reason === 'pre_update') return 'Antes de atualizar';
    if (reason === 'pre_migration') return 'Antes de migrar';
    if (reason === 'pre_restore') return 'Antes de restaurar';
    return 'Automático';
  }

  formatBackupSize(bytes: number): string {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    return `${Math.max(1, Math.round(bytes / 1_000))} KB`;
  }

  useRecommendedAi(): void {
    this.setAiMode('local');
    this.aiSelectedProfile = this.ai.localStatus().recommended.id;
  }

  aiRecommendationReason(): string {
    const hardware = this.ai.localStatus().hardware;
    const profile = this.ai.localStatus().recommended;
    if (!hardware.totalMemoryGb) return `${profile.name} oferece o melhor equilíbrio estimado para este dispositivo.`;
    const gpu = hardware.gpuMemoryGb ? ` e ${hardware.gpuMemoryGb.toFixed(1)} GB de memória gráfica` : '';
    return `Recomendação calculada com ${hardware.totalMemoryGb.toFixed(1)} GB de RAM, ${hardware.logicalCores} processadores lógicos${gpu} e pontuação ${hardware.score}/100.`;
  }

  saveWriterGuidance(): void {
    this.ai.setWriterGuidance(this.aiWriterGuidance);
    this.showInfo('Perfil criativo salvo somente neste dispositivo.');
  }

  clearAiLearning(): void {
    const universeId = this.appState.activeUniverseId();
    if (!universeId || !window.confirm('Esquecer as decisões de IA registradas neste universo?')) return;
    this.ai.forgetCreativeMemory(universeId);
    this.showInfo('Memória de decisões deste universo removida.');
  }

  aiMemoryCount(): number {
    const universeId = this.appState.activeUniverseId();
    return universeId ? this.ai.creativeMemory().filter((item) => item.scope === universeId).length : 0;
  }

  saveAiSettings(): void {
    try {
      if (this.aiMode === 'local') { void this.activateLocalAi(); return; }
      this.ai.configure({ mode: this.aiMode, endpoint: this.aiEndpoint, model: this.aiModel }, this.aiApiKey);
      this.aiEndpoint = this.ai.settings().endpoint;
      this.showInfo(this.aiMode === 'off' ? 'Assistência por IA desativada.' : 'API própria configurada para esta sessão.');
    } catch (error) { this.reportError('A configuração de IA não é válida.', error); }
  }
  openAiAssistant(request: AiWritingRequest | null = null): void {
    if (!this.ai.enabled()) { this.showInfo('Configure a IA nas preferências antes de usar o assistente.'); return; }
    if (!request?.instruction.trim()) return;
    this.aiWritingRequest.set(request);
    this.aiPrompt = request.instruction;
    this.aiResponse.set(''); this.aiError.set('');
    void this.runAiAssistant();
  }
  async runAiAssistant(): Promise<void> {
    if (!this.aiPrompt.trim() || this.aiBusy()) return;
    this.aiBusy.set(true); this.aiResponse.set(''); this.aiError.set('');
    try {
      const selected = this.aiWritingRequest()?.selection?.text;
      const chapterText = selected || this.contentParagraphs(this.editorContent()).join('\n').slice(-12_000);
      const characters = this.characterEntities().slice(0, 30).map((character) => `- ${character.name}`).join('\n');
      const context = this.buildUniverseAiContext([
        `CAPÍTULO: ${this.editorTitle().trim() || 'Sem título'}`,
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
    }
    finally { this.aiBusy.set(false); }
  }
  applyAiResponse(mode: 'replace' | 'insert'): void {
    const response = this.aiResponse().trim();
    if (!response) return;
    const request = this.aiWritingRequest(); const selection = request?.selection;
    const universeId = this.appState.activeUniverseId();
    if (universeId) this.ai.remember(universeId, 'writing', `Aceitou a ação “${(request?.instruction || this.aiPrompt).slice(0, 140)}”.`);
    queueMicrotask(() => {
      if (mode === 'replace' && selection) this.writingEditor?.replaceRange(selection.from, selection.to, response);
      else if (request?.insertAt !== undefined) this.writingEditor?.insertAtPosition(request.insertAt, response);
      else this.writingEditor?.insertPlainText(response);
    });
    this.clearAiAssistant();
  }
  clearAiAssistant(): void { this.aiWritingRequest.set(null); this.aiResponse.set(''); this.aiError.set(''); this.aiPrompt = ''; }
  isUniverseSelectedForShare(id: string): boolean { return this.shareSelectedUniverseIds().has(id); }
  toggleShareUniverse(id: string): void { this.toggleShareSelection(this.shareSelectedUniverseIds, id); }
  selectAllShareUniverses(): void {
    this.shareSelectedUniverseIds.set(this.shareSelectedUniverseIds().size === this.universes().length
      ? new Set()
      : new Set(this.universes().map((universe) => universe.id)));
  }
  async startOnlineShareSession(showFeedback = true): Promise<boolean> {
    this.shareBusy.set(true); this.shareProgressMessage.set('Abrindo um túnel seguro e temporário…'); this.errorMessage.set('');
    try {
      this.shareSession.set(await this.onlineShareService.start());
      if (showFeedback) this.showInfo('Compartilhamento temporário disponível enquanto o NarraHub estiver aberto.');
      return true;
    } catch (error) {
      this.reportError('Não foi possível abrir o compartilhamento temporário.', error);
      return false;
    } finally { this.shareBusy.set(false); this.shareProgressMessage.set(''); }
  }
  async stopOnlineShareSession(): Promise<void> {
    this.shareBusy.set(true); this.shareProgressMessage.set('Encerrando links públicos…');
    try {
      await this.syncCollaborationContributions();
      this.shareSession.set(await this.onlineShareService.stop());
      await this.collaborationService.endAllActive('ended');
      this.onlineShares.set([]); this.shareLink.set(''); this.shareExpiresAt.set('');
      await this.loadCollaborationReview();
      this.showInfo('Sessão encerrada. Todos os links temporários foram invalidados.');
    } catch (error) { this.reportError('Não foi possível encerrar o compartilhamento.', error); }
    finally { this.shareBusy.set(false); this.shareProgressMessage.set(''); }
  }
  async createOnlineShare(): Promise<void> {
    if (this.shareSelectionCount() === 0) { this.showInfo('Selecione ao menos um universo para compartilhar.'); return; }
    this.errorMessage.set('');
    if (!(await this.startOnlineShareSession(false))) return;
    this.shareBusy.set(true); this.shareProgressMessage.set('Preparando e criptografando somente os itens selecionados…');
    try {
      await this.saveChapterNow();
      const selectedUniverses = this.universes().filter((universe) => this.isUniverseSelectedForShare(universe.id));
      const sharedUniverses = await Promise.all(selectedUniverses.map((universe) => this.buildSharedUniverse(universe)));
      const title = selectedUniverses.length === 1 ? selectedUniverses[0].name : `${selectedUniverses.length} universos literários`;
      const document: OnlineShareDocument = {
        version: 3,
        kind: 'workspace',
        title,
        permission: this.sharePermission,
        universes: sharedUniverses,
        sharedAt: new Date().toISOString(),
      };
      const created = await this.onlineShareService.create(document, Number(this.shareExpiresInDays));
      this.shareLink.set(created.url); this.shareExpiresAt.set(created.expiresAt);
      this.rememberShare(created.id, created.revokeToken, created.expiresAt, title, created.encryptionKey, this.sharePermission, selectedUniverses.map((item) => item.id));
      await this.collaborationService.saveSession({
        id: created.id, title, permission: this.sharePermission, universeIds: selectedUniverses.map((item) => item.id),
        encryptionKey: created.encryptionKey, revokeToken: created.revokeToken, expiresAt: created.expiresAt,
      });
      await this.loadCollaborationReview();
      this.shareSession.update((status) => ({ ...status, shareCount: status.shareCount + 1 }));
      const copied = await this.copyShareLink(false);
      this.showInfo(copied ? 'Link criptografado criado e copiado.' : 'Link criptografado criado. Copie-o manualmente.');
    } catch (error) { this.reportError('Não foi possível criar o compartilhamento online.', error); }
    finally { this.shareBusy.set(false); this.shareProgressMessage.set(''); }
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
    this.shareBusy.set(true); this.errorMessage.set('');
    try {
      await this.syncCollaborationContributions();
      this.shareSession.set(await this.onlineShareService.revoke(share.id, share.revokeToken));
      await this.collaborationService.endSession(share.id, 'revoked');
      const remaining = this.onlineShares().filter((item) => item.id !== share.id);
      this.onlineShares.set(remaining);
      await this.loadCollaborationReview();
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
    this.updateBusy.set(true); this.updatePhase.set('backing-up'); this.updateProgress.set(0); this.updateError.set(''); this.backupBusy.set(true);
    try {
      await this.saveChapterNow();
      if (this.saveMessage() === 'Erro ao salvar') throw new Error('A atualização foi interrompida porque o capítulo atual não pôde ser salvo.');
      const backup = await this.backupService.create('pre_update');
      const validation = await this.backupService.validate(backup.backupId);
      this.lastBackupValidation.set(validation);
      if (!validation.valid) throw new Error(`A atualização foi interrompida porque o backup de segurança não foi validado. ${validation.errors.join(' ')}`);
      this.backups.set(await this.backupService.list());
      this.databaseHealth.set(validation.databaseHealth);
      this.backupBusy.set(false); this.updatePhase.set('downloading');
      await this.updateService.downloadAndInstall((progress) => this.updateProgress.set(progress));
      await this.updateService.relaunch();
    } catch (error) {
      this.updatePhase.set('error'); this.updateError.set(error instanceof Error ? error.message : String(error));
      this.reportError('Não foi possível instalar a atualização.', error);
    } finally { this.updateBusy.set(false); this.backupBusy.set(false); }
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
  private async deleteStoryRecord(id: string): Promise<void> {
    const wasActive = this.activeStory()?.id === id;
    if (wasActive) this.clearWritingSelection();
    await this.storyService.delete(id);
    const universeId = this.appState.activeUniverseId();
    if (!universeId) return;
    this.stories.set(await this.storyService.listByUniverse(universeId));
    if (wasActive && this.stories().length) await this.selectStory(this.stories()[0]);
  }
  private async deleteBookRecord(id: string): Promise<void> {
    const story = this.activeStory();
    const wasActive = this.activeBook()?.id === id;
    if (wasActive) {
      this.activeBook.set(null); this.activeChapter.set(null); this.chapters.set([]);
      this.appState.activeBookId.set(null); this.appState.activeChapterId.set(null);
      this.editorTitle.set(''); this.editorContent.set('');
    }
    await this.bookService.delete(id);
    if (!story) return;
    this.books.set(await this.bookService.listByStory(story.id));
    if (wasActive && this.books().length) await this.selectBook(this.books()[0]);
  }
  private async deleteChapterRecord(id: string): Promise<void> {
    const book = this.activeBook();
    const wasActive = this.activeChapter()?.id === id;
    if (wasActive) {
      this.activeChapter.set(null); this.appState.activeChapterId.set(null);
      this.editorTitle.set(''); this.editorContent.set(''); this.saveMessage.set('');
    }
    await this.chapterService.delete(id);
    if (!book) return;
    this.chapters.set(await this.chapterService.listByBook(book.id));
    if (wasActive && this.chapters().length) await this.selectChapter(this.chapters()[0]);
  }
  private async refreshUniverseStats(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const updated = await this.universeStore.refreshStats(id);
    if (updated && this.appState.activeUniverseId() === id) this.appState.activeUniverse.set(updated);
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
  private buildUniverseAiContext(focus: string): string {
    const universe = this.appState.activeUniverse();
    if (!universe) return focus;
    const canon = this.entities().slice(0, 40).map((entity) => {
      const description = (entity.summary || entity.description).trim().replace(/\s+/gu, ' ').slice(0, 240);
      return `- ${entity.type}: ${entity.name}${description ? ` — ${description}` : ''}`;
    }).join('\n');
    return [
      `UNIVERSO: ${universe.name}`,
      universe.description ? `PREMISSA: ${universe.description.slice(0, 1_500)}` : '',
      canon ? `CÂNONE CADASTRADO:\n${canon}` : 'CÂNONE CADASTRADO: ainda vazio',
      this.ai.memoryContext(universe.id),
      focus,
    ].filter(Boolean).join('\n\n');
  }

  private normalizeSearch(value: string): string { return value.normalize('NFD').replace(/[\u0300-\u036f]/gu, '').toLocaleLowerCase('pt-BR').trim(); }
  private setExpanded(target: typeof this.expandedStoryIds, id: string, expanded: boolean): void {
    target.update((current) => { const next = new Set(current); if (expanded) next.add(id); else next.delete(id); return next; });
  }
  private async loadChapterMetadata(chapterId: string): Promise<void> {
    const tags = await this.metadataService.listOwnerTags('chapter', chapterId);
    if (this.activeChapter()?.id !== chapterId) return;
    this.chapterTags.set(tags);
  }

  private async reloadMetadataTarget(): Promise<void> {
    const target = this.metadataTarget(); const universeId = target?.type === 'universe' ? target.id : this.appState.activeUniverseId(); if (!universeId || !target) return;
    const [tags, ownerTags] = await Promise.all([
      this.metadataService.listTags(universeId), this.metadataService.listOwnerTags(target.type, target.id),
    ]);
    this.metadataTags.set(tags); this.metadataOwnerTags.set(ownerTags);
    if (target.type === 'chapter' && target.id === this.activeChapter()?.id) this.chapterTags.set(ownerTags);
    if (this.appState.activeUniverseId() === universeId) await this.refreshWorkspacePreviewTags(universeId);
    if (target.type === 'universe') await this.refreshLibraryPreviewTags();
  }

  private async refreshLibraryPreviewTags(): Promise<void> {
    const universeIds = this.universes().map((universe) => universe.id);
    const assignments = await this.metadataService.listAssignments(universeIds, ['universe']);
    this.libraryPreviewTags.set(this.groupTagAssignments(assignments));
  }

  private async refreshWorkspacePreviewTags(universeId: string): Promise<void> {
    const assignments = await this.metadataService.listAssignments([universeId]);
    if (this.appState.activeUniverseId() === universeId) this.workspacePreviewTags.set(this.groupTagAssignments(assignments));
  }

  private groupTagAssignments(assignments: ContentTagAssignment[]): Record<string, ContentTag[]> {
    const grouped: Record<string, ContentTag[]> = {};
    for (const assignment of assignments) {
      const key = this.tagPreviewKey(assignment.owner_type, assignment.owner_id);
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

  private tagPreviewKey(type: MetadataOwnerType, id: string): string { return `${type}:${id}`; }

  setWorkspaceNavigation(navId: Exclude<AppNavigationId, 'inicio' | 'configuracoes'>): void {
    this.activeNav.set(navId);
    void this.navigation.navigate(navId, this.appState.activeUniverseId());
  }

  private async restoreRoute(route: AppRouteState): Promise<void> {
    if (this.restoringRoute) return;
    if (route.navId === 'inicio' && this.activeNav() === 'inicio' && this.appState.currentView() === 'home') return;
    if (route.navId === 'configuracoes' && this.activeNav() === 'configuracoes') return;
    if (route.universeId && route.universeId === this.appState.activeUniverseId() && route.navId === this.activeNav()) return;

    this.restoringRoute = true;
    try {
      if (route.navId === 'inicio') {
        await this.returnToLibrary(false);
        return;
      }
      if (route.navId === 'configuracoes') {
        this.openSettings(false);
        return;
      }

      const universe = this.universes().find((item) => item.id === route.universeId);
      if (!universe) {
        await this.navigation.navigate('inicio', null);
        this.showInfo('O universo desta rota não existe mais neste banco local.');
        return;
      }
      if (this.appState.activeUniverseId() !== universe.id) await this.openUniverse(universe, false);
      const navItem = this.navItems.find((item) => item.id === route.navId);
      if (navItem) await this.selectNav(navItem, false);
    } finally {
      this.restoringRoute = false;
    }
  }

  private clearWritingSelection(): void { this.activeStory.set(null); this.activeBook.set(null); this.activeChapter.set(null); this.books.set([]); this.chapters.set([]); this.editorTitle.set(''); this.editorContent.set(''); this.chapterSummary.set(''); this.chapterTags.set([]); }
  private resetWorkspaceData(): void { this.clearWritingSelection(); this.stories.set([]); this.universeBooks.set([]); this.universeChapters.set([]); this.entityStore.reset(); this.mentionOccurrences.set([]); this.timelineStore.reset(); this.historyStore.reset(); this.planning.set([]); this.relations.set([]); this.workspacePreviewTags.set({}); this.expandedStoryIds.set(new Set()); this.expandedBookIds.set(new Set()); }
  async loadCollaborationReview(): Promise<void> {
    const [sessions, contributions] = await Promise.all([
      this.collaborationService.listSessions(),
      this.collaborationService.listContributions(),
    ]);
    this.collaborationSessions.set(sessions);
    this.collaborationContributions.set(contributions);
    const selected = this.selectedCollaborationSessionId();
    if (!selected || !sessions.some((session) => session.id === selected)) {
      this.selectedCollaborationSessionId.set(sessions[0]?.id ?? null);
    }
  }

  async syncCollaborationContributions(): Promise<void> {
    if (!isTauri() || !this.onlineShares().length) return;
    let changed = false;
    for (const share of this.onlineShares()) {
      try {
        const contributions = await this.onlineShareService.contributions(share.id, share.revokeToken, share.encryptionKey, share.lastSequence);
        let lastSequence = share.lastSequence;
        for (const item of contributions) {
          lastSequence = Math.max(lastSequence, item.sequence);
          const payload = item.payload;
          const allowed = payload.contributionKind === 'note'
            ? share.permission !== 'view'
            : share.permission === 'edit';
          if (!allowed || !share.universeIds.includes(payload.universeId)) continue;
          changed = (await this.collaborationService.storeContribution(share.id, item.sequence, {
            id: payload.id,
            contributor: payload.contributor,
            kind: payload.contributionKind,
            universeId: payload.universeId,
            targetType: payload.targetType,
            targetId: payload.targetId,
            targetLabel: payload.targetLabel,
            field: payload.field,
            originalValue: payload.originalValue,
            proposedValue: payload.proposedValue,
            message: payload.message,
            createdAt: payload.createdAt,
          })) || changed;
        }
        if (lastSequence !== share.lastSequence) {
          this.onlineShares.update((items) => items.map((item) => item.id === share.id ? { ...item, lastSequence } : item));
        }
      } catch (error) {
        console.warn(`[NarraHub] Não foi possível buscar contribuições da sessão ${share.id}.`, error);
      }
    }
    if (changed) await this.loadCollaborationReview();
  }

  async reviewCollaboration(item: CollaborationContribution, decision: 'approved' | 'rejected'): Promise<void> {
    try {
      await this.collaborationService.review(item.id, decision);
      await this.loadCollaborationReview();
      if (decision === 'approved') await this.refreshAfterCollaborationReview(item.universe_id);
      this.showInfo(decision === 'approved' ? 'Alteração aprovada e aplicada ao banco local.' : 'Alteração rejeitada e preservada no histórico da sessão.');
    } catch (error) { this.reportError('Não foi possível revisar a alteração colaborativa.', error); }
  }

  async approveAllCollaboration(sessionId: string): Promise<void> {
    try {
      const count = await this.collaborationService.approveAll(sessionId);
      await this.loadCollaborationReview();
      await this.refreshAfterCollaborationReview(this.appState.activeUniverseId() || '');
      this.showInfo(`${count} alteração(ões) aprovada(s) e aplicada(s).`);
    } catch (error) { this.reportError('Não foi possível aprovar as alterações em lote.', error); }
  }

  selectCollaborationSession(id: string): void { this.selectedCollaborationSessionId.set(id); }

  sharePermissionLabel(permission: SharePermission): string {
    return permission === 'edit' ? 'Pode propor edições' : permission === 'comment' ? 'Somente anotações' : 'Somente leitura';
  }

  contributionFieldLabel(field: string): string {
    if (field.startsWith('attribute:')) return field.slice('attribute:'.length);
    return ({ name: 'Nome', description: 'Descrição', title: 'Título', content: 'Texto', summary: 'Resumo', canon_status: 'Estado canônico' } as Record<string, string>)[field] || field;
  }

  private async buildSharedUniverse(universe: UniverseWithStats): Promise<SharedUniverse> {
    const [chapters, entities] = await Promise.all([
      this.shareIncludeChapters() ? this.chapterService.listByUniverse(universe.id) : Promise.resolve([]),
      this.shareIncludeEntities() ? this.entityStore.listSnapshot(universe.id) : Promise.resolve([]),
    ]);
    const details = this.shareIncludeEntities()
      ? (await Promise.all(entities.map((entity) => this.entityStore.getDetailsSnapshot(entity.id)))).filter((entity): entity is EntityWithDetails => !!entity)
      : [];
    return {
      id: universe.id,
      name: universe.name,
      description: universe.description,
      coverImage: await this.prepareShareImage(universe.cover_image),
      chapters: chapters.map((chapter) => ({
        id: chapter.id,
        title: chapter.title,
        content: chapter.content,
        summary: chapter.summary,
        storyName: chapter.story_name,
        bookName: chapter.book_name,
      })),
      entities: await Promise.all(details.map(async (entity) => ({
        id: entity.id,
        type: entity.type,
        name: entity.name,
        summary: entity.summary,
        description: entity.description,
        image: await this.prepareShareImage(entity.image),
        canonStatus: entity.canon_status,
        attributes: entity.attributes.filter((attribute) => attribute.value.trim()).map((attribute) => ({ key: attribute.key, value: attribute.value })),
      }))),
    };
  }

  private async refreshAfterCollaborationReview(universeId: string): Promise<void> {
    await this.loadUniverses();
    if (!universeId || this.appState.activeUniverseId() !== universeId) return;
    const [universe, chapters] = await Promise.all([
      this.universeStore.get(universeId),
      this.chapterService.listByUniverse(universeId),
      this.entityStore.refreshAfterExternalChange(universeId),
    ]);
    if (universe) this.appState.activeUniverse.update((active) => active ? { ...active, ...universe } : active);
    this.universeChapters.set(chapters);
    const activeChapterId = this.activeChapter()?.id;
    if (activeChapterId) {
      const chapter = await this.chapterService.get(activeChapterId);
      if (chapter) { this.activeChapter.set(chapter); this.editorTitle.set(chapter.title); this.editorContent.set(chapter.content); }
    }
  }

  private toggleShareSelection(target: typeof this.shareSelectedUniverseIds, id: string): void {
    target.update((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }
  private async prepareShareImage(dataUrl: string): Promise<string> {
    if (!dataUrl || dataUrl.length <= 180_000) return dataUrl;
    return new Promise((resolve) => {
      const image = new Image();
      image.onload = () => {
        const scale = Math.min(1, 720 / Math.max(image.naturalWidth, image.naturalHeight));
        const canvas = document.createElement('canvas');
        canvas.width = Math.max(1, Math.round(image.naturalWidth * scale));
        canvas.height = Math.max(1, Math.round(image.naturalHeight * scale));
        canvas.getContext('2d')?.drawImage(image, 0, 0, canvas.width, canvas.height);
        resolve(canvas.toDataURL('image/webp', 0.76));
      };
      image.onerror = () => resolve('');
      image.src = dataUrl;
    });
  }
  private rememberShare(id: string, revokeToken: string, expiresAt: string, title: string, encryptionKey: string, permission: SharePermission, universeIds: string[]): void {
    const shares = [{ id, revokeToken, expiresAt, title, encryptionKey, permission, universeIds, lastSequence: 0 }, ...this.onlineShares().filter((item) => item.id !== id)].slice(0, 50);
    this.onlineShares.set(shares);
  }
  private countWords(content: string): number { const normalized = content.replace(/<[^>]+>/g, ' ').trim(); return normalized ? normalized.split(/\s+/u).length : 0; }
  private showInfo(message: string): void { this.infoMessage.set(message); if (this.infoTimer) clearTimeout(this.infoTimer); this.infoTimer = setTimeout(() => this.infoMessage.set(''), 4200); }
  private reportError(message: string, error: unknown): void { console.error(`[NarraHub] ${message}`, error); this.errorMessage.set(`${message} ${error instanceof Error ? error.message : String(error)}`); }
}
