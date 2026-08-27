import { CommonModule } from '@angular/common';
import { Component, HostListener, OnDestroy, OnInit, ViewChild, computed, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  BookOption, ChapterOption, ContentTag, ContentTagAssignment, Entity, EntityWithDetails,
  MentionOccurrence, MetadataOwnerType, PlanningItem, RelationCard, UniverseWithStats,
} from './core/models';
import { BackupManifest } from './core/services/backup.service';
import { AiService } from './core/services/ai.service';
import { DatabaseService } from './core/services/database.service';
import { AppNavigationId, AppRouteState } from './core/navigation/app-navigation';
import { AppNavigationService } from './core/navigation/app-navigation.service';
import { MentionService } from './core/services/mention.service';
import { MetadataService } from './core/services/metadata.service';
import { OnlineShareDocument, SharedUniverse } from './core/services/online-share.service';
import { PlanningService } from './core/services/planning.service';
import { WorkspaceService } from './core/services/workspace.service';
import { AppState } from './core/state/app.state';
import { ShareCreateRequest, ShareModalComponent } from './features/collaboration/share-modal/share-modal.component';
import { CollaborationStore } from './features/collaboration/state/collaboration.store';
import { ConnectionsGraphComponent } from './features/connections/connections-graph.component';
import { EntitiesPageComponent, EntityMutationKind } from './features/entities/entities-page/entities-page.component';
import { EntityHubType, EntityStore } from './features/entities/state/entity.store';
import { PlanningBoardComponent } from './features/planning/planning-board.component';
import { HistoryPageComponent } from './features/history/history-page.component';
import { HistoryStore } from './features/history/state/history.store';
import { LibraryPageComponent } from './features/library/library-page.component';
import { UniverseStore } from './features/library/state/universe.store';
import { ManuscriptMetadataRequest, WritingPageComponent } from './features/manuscript/writing-page.component';
import { ManuscriptStore } from './features/manuscript/state/manuscript.store';
import { SettingsPageComponent } from './features/settings/settings-page.component';
import { SettingsStore } from './features/settings/state/settings.store';
import { TimelinePageComponent } from './features/timeline/timeline-page.component';
import { TimelineStore } from './features/timeline/state/timeline.store';
import { AppShellComponent } from './shell/app-shell/app-shell.component';
import { TitlebarComponent } from './shell/titlebar/titlebar.component';
import { SidebarNavItem, UniverseSidebarComponent } from './shell/universe-sidebar/universe-sidebar.component';

interface GlobalSearchResult {
  id: string;
  kind: 'story' | 'book' | 'chapter' | 'entity' | 'timeline' | 'planning';
  label: string;
  context: string;
  icon: string;
}

interface PendingRelationDelete {
  id: string;
  label: string;
}

interface MetadataTarget {
  type: MetadataOwnerType;
  id: string;
  name: string;
}

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    AppShellComponent,
    TitlebarComponent,
    UniverseSidebarComponent,
    LibraryPageComponent,
    EntitiesPageComponent,
    ConnectionsGraphComponent,
    PlanningBoardComponent,
    HistoryPageComponent,
    SettingsPageComponent,
    ShareModalComponent,
    TimelinePageComponent,
    WritingPageComponent,
  ],
  templateUrl: './app.html',
  styleUrl: './app.css',
})
export class App implements OnInit, OnDestroy {
  readonly Math = Math;
  readonly appState = inject(AppState);
  readonly ai = inject(AiService);
  private readonly db = inject(DatabaseService);
  private readonly universeStore = inject(UniverseStore);
  private readonly collaborationStore = inject(CollaborationStore);
  private readonly entityStore = inject(EntityStore);
  private readonly manuscriptStore = inject(ManuscriptStore);
  private readonly mentionService = inject(MentionService);
  private readonly metadataService = inject(MetadataService);
  private readonly planningService = inject(PlanningService);
  private readonly workspaceService = inject(WorkspaceService);
  private readonly settingsStore = inject(SettingsStore);
  private readonly historyStore = inject(HistoryStore);
  private readonly timelineStore = inject(TimelineStore);
  private readonly navigation = inject(AppNavigationService);

  readonly searchQuery = signal('');
  readonly activeNav = signal('inicio');
  readonly universes = this.universeStore.universes;
  readonly entities = this.entityStore.entities;
  readonly entityFilter = this.entityStore.filter;
  readonly mentionOccurrences = signal<MentionOccurrence[]>([]);
  readonly timeline = this.timelineStore.events;
  readonly planning = signal<PlanningItem[]>([]);
  readonly relations = signal<RelationCard[]>([]);
  readonly activeStory = this.manuscriptStore.activeStory;
  readonly activeBook = this.manuscriptStore.activeBook;
  readonly activeChapter = this.manuscriptStore.activeChapter;
  readonly saveMessage = this.manuscriptStore.saveMessage;
  readonly isSaving = this.manuscriptStore.isSaving;
  readonly inspectorOpen = this.manuscriptStore.inspectorOpen;
  readonly isLoading = signal(true);
  readonly isFocusMode = signal(false);
  readonly errorMessage = signal('');
  readonly infoMessage = signal('');
  readonly updateBusy = this.settingsStore.updateBusy;
  readonly updatePhase = this.settingsStore.updatePhase;
  readonly updateInfo = this.settingsStore.updateInfo;
  readonly updateProgress = this.settingsStore.updateProgress;
  readonly updatePromptDismissed = this.settingsStore.updatePromptDismissed;
  readonly lastOpenedUniverseId = signal<string | null>(localStorage.getItem('narrahub.lastUniverseId'));
  readonly pendingRelationDelete = signal<PendingRelationDelete | null>(null);
  readonly renamingUniverse = signal(false);
  readonly metadataTarget = signal<MetadataTarget | null>(null);
  readonly metadataTags = signal<ContentTag[]>([]);
  readonly metadataOwnerTags = signal<ContentTag[]>([]);
  readonly libraryPreviewTags = signal<Record<string, ContentTag[]>>({});
  readonly workspacePreviewTags = signal<Record<string, ContentTag[]>>({});

  newRelationSource = '';
  newRelationTarget = '';
  newRelationLabel = '';
  newTagName = '';
  newTagColor = '#7d3650';
  renameValue = '';

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

  readonly globalSearchResults = computed<GlobalSearchResult[]>(() => {
    if (this.appState.currentView() !== 'workspace') return [];
    const query = this.normalizeSearch(this.searchQuery());
    if (query.length < 2) return [];
    const matches = (value: string) => this.normalizeSearch(value).includes(query);
    const results: GlobalSearchResult[] = [];
    for (const story of this.manuscriptStore.stories()) if (matches(`${story.name} ${story.description}`)) results.push({ id: story.id, kind: 'story', label: story.name, context: 'História', icon: '⌂' });
    for (const book of this.manuscriptStore.universeBooks()) if (matches(`${book.name} ${book.description} ${book.story_name}`)) results.push({ id: book.id, kind: 'book', label: book.name, context: `Livro · ${book.story_name}`, icon: '▱' });
    for (const chapter of this.manuscriptStore.universeChapters()) if (matches(`${chapter.title} ${chapter.content} ${chapter.book_name} ${chapter.story_name}`)) results.push({ id: chapter.id, kind: 'chapter', label: chapter.title, context: `${chapter.story_name} · ${chapter.book_name}`, icon: '▤' });
    for (const entity of this.entities()) if (matches(`${entity.name} ${entity.summary} ${entity.description} ${entity.type}`)) results.push({ id: entity.id, kind: 'entity', label: entity.name, context: entity.type, icon: entity.name.charAt(0).toUpperCase() });
    for (const event of this.timeline()) if (matches(`${event.title} ${event.description} ${event.display_date || event.start_date}`)) results.push({ id: event.id, kind: 'timeline', label: event.title, context: 'Linha do tempo', icon: '◷' });
    for (const item of this.planning()) if (matches(`${item.title} ${item.description} ${item.status}`)) results.push({ id: item.id, kind: 'planning', label: item.title, context: `Planejamento · ${item.status}`, icon: '☑' });
    return results.slice(0, 24);
  });

  @ViewChild(WritingPageComponent) private writingPage?: WritingPageComponent;
  @ViewChild(PlanningBoardComponent) private planningBoard?: PlanningBoardComponent;
  @ViewChild(TimelinePageComponent) private timelinePage?: TimelinePageComponent;
  @ViewChild(EntitiesPageComponent) readonly entitiesPage?: EntitiesPageComponent;

  private infoTimer: ReturnType<typeof setTimeout> | null = null;
  private collaborationTimer: ReturnType<typeof setInterval> | null = null;
  private workspaceEpoch = 0;
  private restoringRoute = false;

  constructor() {
    // Hook cross-domain: menções (Knowledge, ainda não extraído) e estatísticas
    // do universo dependem de um capítulo ter sido salvo, mas o ManuscriptStore
    // não pode conhecer MentionService/UniverseStore. Ver o comentário do campo
    // no próprio store.
    this.manuscriptStore.onChapterPersisted = (chapterId, content) => {
      void this.mentionService.syncChapterMentions(chapterId, this.entityIdsInContent(content, this.entities()));
      void Promise.all([this.refreshUniverseStats(), this.refreshMentionOccurrences()]);
    };
    effect(() => {
      const route = this.navigation.route();
      if (!this.isLoading()) void this.restoreRoute(route);
    });
  }

  async ngOnInit(): Promise<void> {
    await this.ai.initialize().catch((error) => {
      console.error('[NarraHub] Não foi possível inicializar o gerenciador da IA local.', error);
    });
    if (!isTauri()) { this.isLoading.set(false); return; }
    try {
      await this.db.init();
      await this.loadUniverses();
      await this.collaborationStore.refreshShareStatus();
      await this.collaborationStore.loadReview();
      this.collaborationTimer = setInterval(() => void this.collaborationStore.syncIncoming(), 2500);
      await this.settingsStore.primeCurrentVersion();
      if (await this.settingsStore.isUpdateConfigured()) setTimeout(() => void this.checkForUpdates(true), 1800);
    } catch (error) {
      this.reportError('Não foi possível abrir o banco local do NarraHub.', error);
    } finally { this.isLoading.set(false); }
  }

  ngOnDestroy(): void {
    if (this.infoTimer) clearTimeout(this.infoTimer);
    if (this.collaborationTimer) clearInterval(this.collaborationTimer);
    this.settingsStore.dispose();
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
    await this.collaborationStore.syncIncoming();
    await this.collaborationStore.endAllActiveQuietly();
    await this.collaborationStore.stopShareQuietly();
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

  requestDeleteRelation(id: string, label: string, event?: Event): void {
    event?.stopPropagation();
    this.pendingRelationDelete.set({ id, label });
    this.appState.openModal('delete-item');
  }

  async confirmDeleteRelation(): Promise<void> {
    const pending = this.pendingRelationDelete();
    if (!pending) return;
    try {
      await this.workspaceService.deleteRelation(pending.id);
      this.pendingRelationDelete.set(null);
      this.appState.closeModal();
      await Promise.all([this.refreshUniverseStats(), this.loadRelations()]);
      this.showInfo('Ligação excluída do banco local.');
    } catch (error) { this.reportError(`Não foi possível excluir ${pending.label}.`, error); }
  }

  beginRenameUniverse(): void {
    const universe = this.appState.activeUniverse();
    if (!universe) return;
    this.renameValue = universe.name;
    this.renamingUniverse.set(true);
    this.appState.openModal('rename-item');
  }

  async confirmRenameUniverse(): Promise<void> {
    const universe = this.appState.activeUniverse();
    const name = this.renameValue.trim();
    if (!universe || !name) return;
    try {
      await this.saveChapterNow();
      this.appState.activeUniverse.update((item) => item?.id === universe.id ? { ...item, name, updated_at: this.db.now() } : item);
      await this.universeStore.update(universe.id, { name });
      this.renamingUniverse.set(false); this.renameValue = ''; this.appState.closeModal();
      this.showInfo('Universo renomeado.');
    } catch (error) { this.reportError(`Não foi possível renomear ${universe.name}.`, error); }
  }

  async returnToLibrary(updateRoute = true): Promise<void> {
    await this.saveChapterNow(); this.workspaceEpoch += 1; this.appState.goHome(); this.activeNav.set('inicio'); this.searchQuery.set(''); this.resetWorkspaceData();
    if (updateRoute) await this.navigation.navigate('inicio', null);
  }

  openSettings(updateRoute = true): void { this.searchQuery.set(''); this.activeNav.set('configuracoes'); this.appState.openSettings(); void this.settingsStore.refreshBackupStatus(); if (updateRoute) void this.navigation.navigate('configuracoes', null); }

  openShareModal(): void {
    if (!this.appState.activeUniverse()) { this.showInfo('Abra um universo antes de compartilhar.'); return; }
    // O link/progresso ficam no CollaborationStore (compartilhado com a aba
    // Configurações > Compartilhar), então precisam ser zerados aqui: o
    // ShareModalComponent nasce de novo a cada abertura, mas o store não.
    this.collaborationStore.shareLink.set('');
    this.collaborationStore.shareExpiresAt.set('');
    this.collaborationStore.shareProgressMessage.set('');
    this.appState.openModal('share-content');
  }

  beginCreateChapter(): void { this.writingPage?.openCreateChapter(); }
  toggleInspector(): void { this.manuscriptStore.toggleInspector(); }
  manuscriptStories() { return this.manuscriptStore.stories(); }
  manuscriptChapters() { return this.manuscriptStore.universeChapters(); }

  async loadWorkspaceData(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const epoch = this.workspaceEpoch;
    try {
      const [, , , planning, tagAssignments] = await Promise.all([
        this.entityStore.load(id, true),
        this.timelineStore.load(id),
        this.manuscriptStore.load(id),
        this.planningService.list(id),
        this.metadataService.listAssignments([id]),
      ]);
      if (epoch !== this.workspaceEpoch || this.appState.activeUniverseId() !== id) return;
      this.planning.set(planning);
      this.workspacePreviewTags.set(this.groupTagAssignments(tagAssignments));
      void this.rebuildMentionIndex(id, this.manuscriptStore.universeChapters(), this.entityStore.entities());
    } catch (error) { this.reportError('Não foi possível carregar os dados do universo.', error); }
  }

  async openBookOption(book: BookOption): Promise<void> {
    if (await this.manuscriptStore.openBookOption(book)) { this.setWorkspaceNavigation('escrita'); this.appState.openEditor(); }
  }

  async openChapterOption(option: ChapterOption): Promise<void> {
    if (await this.manuscriptStore.openChapterOption(option)) { this.setWorkspaceNavigation('escrita'); this.appState.openEditor(); }
  }

  private async saveChapterNow(): Promise<void> { await this.manuscriptStore.saveNow(); }

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

  onManuscriptEntityOpen(entity: Entity): void {
    this.setWorkspaceNavigation('entidades');
    this.selectEntityTab(entity.type as EntityHubType);
    void this.openEntitySheet(entity);
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
        this.manuscriptStore.refreshUniverseLists(),
        this.loadRelations(),
        this.refreshMentionOccurrences(),
      ]);
    }
  }

  onEntityInfo(message: string): void { this.showInfo(message); }

  onEntityFailure(message: string): void { this.reportError(message, message); }

  onManuscriptInfo(message: string): void { this.showInfo(message); }

  onManuscriptFailed(message: string): void { this.reportError(message, message); }

  onManuscriptMetadata(request: ManuscriptMetadataRequest): void {
    void this.openMetadata(request.type, request.id, request.name);
  }

  async openGlobalSearchResult(result: GlobalSearchResult): Promise<void> {
    this.searchQuery.set('');
    if (result.kind === 'story') {
      const opened = await this.manuscriptStore.openStoryById(result.id);
      if (opened) { this.setWorkspaceNavigation('escrita'); this.appState.openEditor(); }
    } else if (result.kind === 'book') {
      const book = this.manuscriptStore.universeBooks().find((item) => item.id === result.id); if (book) await this.openBookOption(book);
    } else if (result.kind === 'chapter') {
      const chapter = this.manuscriptStore.universeChapters().find((item) => item.id === result.id); if (chapter) await this.openChapterOption(chapter);
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

  async loadPlanning(): Promise<void> { const id = this.appState.activeUniverseId(); if (!id) return; const data = await this.planningService.list(id); if (this.appState.activeUniverseId() === id) this.planning.set(data); }
  beginCreatePlanning(): void { this.planningBoard?.openCreate(); }
  beginCreateTimeline(): void { this.timelinePage?.openCreate(); }

  async openPlanningChapter(item: PlanningItem): Promise<void> {
    const option = this.manuscriptStore.universeChapters().find((chapter) => chapter.id === item.chapter_id);
    if (option) await this.openChapterOption(option);
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

  onSettingsInfo(message: string): void { this.showInfo(message); }
  onSettingsFailed(message: string): void { this.reportError(message, message); }

  async checkForUpdates(silent = false): Promise<void> {
    const result = await this.settingsStore.checkForUpdates(silent);
    if (result.message) this.showInfo(result.message);
  }

  async installUpdate(): Promise<void> {
    await this.saveChapterNow();
    if (this.saveMessage() === 'Erro ao salvar') {
      this.reportError('Não foi possível instalar a atualização.', new Error('A atualização foi interrompida porque o capítulo atual não pôde ser salvo.'));
      return;
    }
    const result = await this.settingsStore.installUpdate();
    if (!result.ok) this.reportError('Não foi possível instalar a atualização.', new Error(result.error || ''));
  }

  dismissUpdatePrompt(): void { this.settingsStore.dismissUpdatePrompt(); }

  async createManualBackup(): Promise<void> {
    await this.saveChapterNow();
    const result = await this.settingsStore.createBackup('manual');
    if (result.ok) this.showInfo('Backup local criado e validado.');
    else this.reportError('Não foi possível criar um backup válido.', new Error(result.error || ''));
  }

  async prepareRestoreBackup(backup: BackupManifest): Promise<void> {
    if (this.collaborationStore.shareSession().running || this.settingsStore.syncStatus().running) {
      this.settingsStore.backupError.set('Encerre o compartilhamento e a sincronização antes de restaurar um backup.');
      return;
    }
    await this.saveChapterNow();
    if (this.saveMessage() === 'Erro ao salvar') {
      this.settingsStore.backupError.set('A restauração foi interrompida porque o capítulo atual não pôde ser salvo.');
      return;
    }
    await this.settingsStore.prepareRestore(backup.backupId);
  }

  async createOnlineShare(request: ShareCreateRequest): Promise<void> {
    if (!request.universeIds.length) { this.showInfo('Selecione ao menos um universo para compartilhar.'); return; }
    this.errorMessage.set('');
    const started = await this.collaborationStore.startShareSession();
    if (!started.ok) { this.reportError('Não foi possível abrir o compartilhamento temporário.', new Error(started.error || '')); return; }
    this.collaborationStore.shareBusy.set(true);
    this.collaborationStore.shareProgressMessage.set('Preparando e criptografando somente os itens selecionados…');
    try {
      await this.saveChapterNow();
      const selectedUniverses = this.universes().filter((universe) => request.universeIds.includes(universe.id));
      const sharedUniverses = await Promise.all(selectedUniverses.map((universe) => this.buildSharedUniverse(universe, request.includeChapters, request.includeEntities)));
      const title = selectedUniverses.length === 1 ? selectedUniverses[0].name : `${selectedUniverses.length} universos literários`;
      const document: OnlineShareDocument = {
        version: 3,
        kind: 'workspace',
        title,
        permission: request.permission,
        universes: sharedUniverses,
        sharedAt: new Date().toISOString(),
      };
      const result = await this.collaborationStore.createShare(document, request.expiresInDays, selectedUniverses.map((item) => item.id));
      if (!result.ok) { this.reportError('Não foi possível criar o compartilhamento online.', new Error(result.error || '')); return; }
      const copied = await this.copyToClipboard(this.collaborationStore.shareLink());
      this.showInfo(copied ? 'Link criptografado criado e copiado.' : 'Link criptografado criado. Copie-o manualmente.');
    } catch (error) { this.reportError('Não foi possível criar o compartilhamento online.', error); }
    finally { this.collaborationStore.shareBusy.set(false); this.collaborationStore.shareProgressMessage.set(''); }
  }

  onCollaborationApplied(universeId: string): void {
    void this.refreshAfterCollaborationReview(universeId);
  }

  private async copyToClipboard(text: string): Promise<boolean> {
    if (!text) return false;
    try { await navigator.clipboard.writeText(text); return true; }
    catch (error) { console.warn('[NarraHub] Não foi possível copiar o link automaticamente.', error); return false; }
  }

  formatDate(value: string): string { if (!value) return 'Sem data'; const date = new Date(value.length === 10 ? `${value}T12:00:00` : value); return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' }); }

  private async refreshUniverseStats(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    const updated = await this.universeStore.refreshStats(id);
    if (updated && this.appState.activeUniverseId() === id) this.appState.activeUniverse.set(updated);
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

  private normalizeSearch(value: string): string { return value.normalize('NFD').replace(/[̀-ͯ]/gu, '').toLocaleLowerCase('pt-BR').trim(); }

  private async reloadMetadataTarget(): Promise<void> {
    const target = this.metadataTarget(); const universeId = target?.type === 'universe' ? target.id : this.appState.activeUniverseId(); if (!universeId || !target) return;
    const [tags, ownerTags] = await Promise.all([
      this.metadataService.listTags(universeId), this.metadataService.listOwnerTags(target.type, target.id),
    ]);
    this.metadataTags.set(tags); this.metadataOwnerTags.set(ownerTags);
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

  private resetWorkspaceData(): void {
    this.manuscriptStore.reset();
    this.entityStore.reset();
    this.mentionOccurrences.set([]);
    this.timelineStore.reset();
    this.historyStore.reset();
    this.planning.set([]);
    this.relations.set([]);
    this.workspacePreviewTags.set({});
  }

  private async buildSharedUniverse(universe: UniverseWithStats, includeChapters: boolean, includeEntities: boolean): Promise<SharedUniverse> {
    const [chapters, entities] = await Promise.all([
      includeChapters ? this.manuscriptStore.listChaptersSnapshot(universe.id) : Promise.resolve([]),
      includeEntities ? this.entityStore.listSnapshot(universe.id) : Promise.resolve([]),
    ]);
    const details = includeEntities
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
    const [universe] = await Promise.all([
      this.universeStore.get(universeId),
      this.manuscriptStore.refreshAfterExternalChange(universeId),
      this.entityStore.refreshAfterExternalChange(universeId),
    ]);
    if (universe) this.appState.activeUniverse.update((active) => active ? { ...active, ...universe } : active);
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
  private showInfo(message: string): void { this.infoMessage.set(message); if (this.infoTimer) clearTimeout(this.infoTimer); this.infoTimer = setTimeout(() => this.infoMessage.set(''), 4200); }
  private reportError(message: string, error: unknown): void { console.error(`[NarraHub] ${message}`, error); this.errorMessage.set(`${message} ${error instanceof Error ? error.message : String(error)}`); }
}
