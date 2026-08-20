import { CommonModule } from '@angular/common';
import { Component, HostListener, OnDestroy, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  Book, Chapter, Entity, EntityType, EntityWithDetails, HistoryEntry, PlanningItem,
  PlanningStatus, RelationCard, Story, SyncServerStatus, TimelineEvent, UniverseWithStats,
} from './core/models';
import { BookService } from './core/services/book.service';
import { ChapterService } from './core/services/chapter.service';
import { DatabaseService } from './core/services/database.service';
import { EntityService } from './core/services/entity.service';
import { StoryService } from './core/services/story.service';
import { SyncService } from './core/services/sync.service';
import { ThemePreference, ThemeService } from './core/services/theme.service';
import { UniverseService } from './core/services/universe.service';
import { WorkspaceService } from './core/services/workspace.service';
import { AppState } from './core/state/app.state';

type NavItem = { id: string; label: string; icon: string; needsUniverse: boolean };

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [CommonModule, FormsModule],
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
  private readonly chapterService = inject(ChapterService);
  private readonly entityService = inject(EntityService);
  private readonly workspaceService = inject(WorkspaceService);
  private readonly syncService = inject(SyncService);

  readonly searchQuery = signal('');
  readonly activeNav = signal('inicio');
  readonly universes = signal<UniverseWithStats[]>([]);
  readonly stories = signal<Story[]>([]);
  readonly books = signal<Book[]>([]);
  readonly chapters = signal<Chapter[]>([]);
  readonly entities = signal<Entity[]>([]);
  readonly timeline = signal<TimelineEvent[]>([]);
  readonly planning = signal<PlanningItem[]>([]);
  readonly relations = signal<RelationCard[]>([]);
  readonly history = signal<HistoryEntry[]>([]);
  readonly activeStory = signal<Story | null>(null);
  readonly activeBook = signal<Book | null>(null);
  readonly activeChapter = signal<Chapter | null>(null);
  readonly activeEntity = signal<EntityWithDetails | null>(null);
  readonly editorTitle = signal('');
  readonly editorContent = signal('');
  readonly isLoading = signal(true);
  readonly isSaving = signal(false);
  readonly isFocusMode = signal(false);
  readonly saveMessage = signal('');
  readonly errorMessage = signal('');
  readonly infoMessage = signal('');
  readonly syncBusy = signal(false);
  readonly syncStatus = signal<SyncServerStatus>({ running: false, address: null, pairing_code: null, device_name: 'Meu computador' });

  newUniverseName = '';
  newUniverseDesc = '';
  newStoryName = '';
  newBookName = '';
  newChapterTitle = '';
  newEntityName = '';
  newEntityDescription = '';
  newEntityType: EntityType = 'Personagem';
  newTimelineTitle = '';
  newTimelineDate = '';
  newTimelineDescription = '';
  newPlanningTitle = '';
  newPlanningDescription = '';
  newRelationSource = '';
  newRelationTarget = '';
  newRelationLabel = '';
  deviceName = localStorage.getItem('narrahub.deviceName') || 'Meu computador';
  remoteAddress = '';
  pairingCode = '';

  readonly planningStatuses: PlanningStatus[] = ['IDEIAS', 'PLANEJADO', 'ESCREVENDO', 'REVISAO', 'FINALIZADO'];
  readonly entityTypes: EntityType[] = ['Personagem', 'Lugar', 'Evento', 'Objeto', 'Organização', 'Nota'];
  readonly navItems: NavItem[] = [
    { id: 'inicio', label: 'Início', icon: '⌂', needsUniverse: false },
    { id: 'escrita', label: 'Escrita', icon: '✎', needsUniverse: true },
    { id: 'personagens', label: 'Personagens', icon: '♙', needsUniverse: true },
    { id: 'lugares', label: 'Lugares', icon: '⌖', needsUniverse: true },
    { id: 'eventos', label: 'Eventos', icon: '◇', needsUniverse: true },
    { id: 'conexoes', label: 'Conexões', icon: '⌘', needsUniverse: true },
    { id: 'timeline', label: 'Timeline', icon: '◷', needsUniverse: true },
    { id: 'planejamento', label: 'Planejamento', icon: '☑', needsUniverse: true },
    { id: 'historico', label: 'Histórico', icon: '↶', needsUniverse: true },
    { id: 'configuracoes', label: 'Configurações', icon: '⚙', needsUniverse: false },
  ];

  readonly filteredUniverses = computed(() => {
    const query = this.searchQuery().trim().toLocaleLowerCase('pt-BR');
    if (!query) return this.universes();
    return this.universes().filter((universe) =>
      `${universe.name} ${universe.description}`.toLocaleLowerCase('pt-BR').includes(query),
    );
  });
  readonly totalWords = computed(() => this.universes().reduce((total, universe) => total + universe.stats.total_words, 0));
  readonly totalChapters = computed(() => this.universes().reduce((total, universe) => total + universe.stats.total_chapters, 0));
  readonly wordCount = computed(() => this.countWords(this.editorContent()));

  private saveTimer: ReturnType<typeof setTimeout> | null = null;
  private infoTimer: ReturnType<typeof setTimeout> | null = null;

  async ngOnInit(): Promise<void> {
    if (!isTauri()) { this.isLoading.set(false); return; }
    try {
      await this.db.init();
      await this.loadUniverses();
      this.syncStatus.set(await this.syncService.status());
    } catch (error) {
      this.reportError('Não foi possível abrir o banco local do NarraHub.', error);
    } finally { this.isLoading.set(false); }
  }

  ngOnDestroy(): void {
    if (this.saveTimer) clearTimeout(this.saveTimer);
    if (this.infoTimer) clearTimeout(this.infoTimer);
  }

  @HostListener('document:keydown.control.k', ['$event'])
  focusSearch(event: Event): void {
    event.preventDefault();
    document.querySelector<HTMLInputElement>('.global-search input')?.focus();
  }

  async minimizeWindow(): Promise<void> { if (isTauri()) await getCurrentWindow().minimize(); }
  async toggleMaximizeWindow(): Promise<void> { if (isTauri()) await getCurrentWindow().toggleMaximize(); }
  async closeWindow(): Promise<void> { await this.saveChapterNow(); if (isTauri()) await getCurrentWindow().close(); }
  async toggleFullscreen(): Promise<void> { if (isTauri()) { const win = getCurrentWindow(); await win.setFullscreen(!(await win.isFullscreen())); } }
  async loadUniverses(): Promise<void> { this.universes.set(await this.universeService.list()); }

  async selectNav(item: NavItem): Promise<void> {
    if (item.needsUniverse && !this.appState.activeUniverse()) {
      this.showInfo('Selecione ou crie um universo para abrir esta área.'); return;
    }
    await this.saveChapterNow();
    this.activeNav.set(item.id);
    if (item.id === 'inicio') { this.appState.goHome(); return; }
    if (item.id === 'configuracoes') { this.appState.openSettings(); return; }
    if (item.id === 'escrita') this.appState.openEditor();
    else if (item.id === 'personagens') this.appState.openEntityList('Personagem');
    else if (item.id === 'lugares') this.appState.openEntityList('Lugar');
    else if (item.id === 'eventos') this.appState.openEntityList('Evento');
    else if (item.id === 'conexoes') { this.appState.openGraph(); await this.loadRelations(); }
    else if (item.id === 'timeline') { this.appState.openTimeline(); await this.loadTimeline(); }
    else if (item.id === 'planejamento') { this.appState.openPlanning(); await this.loadPlanning(); }
    else if (item.id === 'historico') { this.appState.openHistory(); await this.loadHistory(); }
  }

  async createUniverse(): Promise<void> {
    const name = this.newUniverseName.trim(); if (!name) return;
    try {
      const created = await this.universeService.create(name, this.newUniverseDesc.trim());
      this.newUniverseName = ''; this.newUniverseDesc = ''; this.appState.closeModal(); await this.loadUniverses();
      const universe = this.universes().find((item) => item.id === created.id); if (universe) await this.openUniverse(universe);
    } catch (error) { this.reportError('Não foi possível criar o universo.', error); }
  }

  async openUniverse(universe: UniverseWithStats): Promise<void> {
    await this.saveChapterNow(); this.appState.openUniverse(universe); this.activeNav.set('escrita'); await this.loadWorkspaceData();
  }

  async deleteUniverse(id: string, event: Event): Promise<void> {
    event.stopPropagation(); const universe = this.universes().find((item) => item.id === id);
    if (!universe || !confirm(`Excluir “${universe.name}” e todo o conteúdo associado?`)) return;
    try {
      await this.universeService.delete(id); await this.loadUniverses();
      if (this.appState.activeUniverseId() === id) { this.appState.goHome(); this.activeNav.set('inicio'); }
    } catch (error) { this.reportError('Não foi possível excluir o universo.', error); }
  }

  async loadWorkspaceData(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return;
    try {
      this.stories.set(await this.storyService.listByUniverse(id)); this.entities.set(await this.entityService.listByUniverse(id));
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
    await this.saveChapterNow(); this.activeStory.set(story); this.appState.activeStoryId.set(story.id);
    this.books.set(await this.bookService.listByStory(story.id));
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
    await this.saveChapterNow(); this.activeBook.set(book); this.appState.activeBookId.set(book.id);
    this.chapters.set(await this.chapterService.listByBook(book.id));
    if (this.chapters().length) await this.selectChapter(this.chapters()[0]); else { this.activeChapter.set(null); this.editorTitle.set(''); this.editorContent.set(''); }
  }

  async createChapter(): Promise<void> {
    const book = this.activeBook(); const title = this.newChapterTitle.trim(); if (!book || !title) return;
    try {
      const chapter = await this.chapterService.create(book.id, title); this.newChapterTitle = ''; this.appState.closeModal();
      this.chapters.set(await this.chapterService.listByBook(book.id)); await this.selectChapter(chapter); await this.refreshUniverseStats();
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
      this.saveMessage.set('Salvo'); await this.refreshUniverseStats();
    } catch (error) { this.saveMessage.set('Erro ao salvar'); this.reportError('O capítulo não foi salvo.', error); }
    finally { this.isSaving.set(false); }
  }

  async createEntity(): Promise<void> {
    const id = this.appState.activeUniverseId(); const name = this.newEntityName.trim(); if (!id || !name) return;
    try {
      await this.entityService.create(id, this.newEntityType, name, this.newEntityDescription.trim());
      this.newEntityName = ''; this.newEntityDescription = ''; this.appState.closeModal(); this.entities.set(await this.entityService.listByUniverse(id)); await this.refreshUniverseStats();
    } catch (error) { this.reportError('Não foi possível criar a entidade.', error); }
  }
  async openEntitySheet(entity: Entity): Promise<void> {
    try { this.activeEntity.set(await this.entityService.getWithDetails(entity.id)); this.appState.openEntitySheet(entity.id); }
    catch (error) { this.reportError('Não foi possível abrir a ficha.', error); }
  }
  visibleEntities(): Entity[] { const filter = this.appState.sidebarEntityFilter(); return filter ? this.entities().filter((entity) => entity.type === filter) : this.entities(); }
  entitySectionTitle(): string { const filter = this.appState.sidebarEntityFilter(); return filter ? (filter === 'Personagem' ? 'Personagens' : filter === 'Lugar' ? 'Lugares' : filter === 'Evento' ? 'Eventos' : `${filter}s`) : 'Entidades'; }

  async loadRelations(): Promise<void> { const id = this.appState.activeUniverseId(); if (id) this.relations.set(await this.workspaceService.listRelations(id)); }
  async createRelation(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id || !this.newRelationSource || !this.newRelationTarget || !this.newRelationLabel.trim()) return;
    if (this.newRelationSource === this.newRelationTarget) { this.showInfo('Escolha duas entidades diferentes.'); return; }
    await this.workspaceService.createRelation(id, this.newRelationSource, this.newRelationTarget, this.newRelationLabel.trim());
    this.newRelationSource = ''; this.newRelationTarget = ''; this.newRelationLabel = ''; this.appState.closeModal(); await this.loadRelations();
  }
  async deleteRelation(id: string): Promise<void> { await this.workspaceService.deleteRelation(id); await this.loadRelations(); }

  async loadTimeline(): Promise<void> { const id = this.appState.activeUniverseId(); if (id) this.timeline.set(await this.workspaceService.listTimeline(id)); }
  async createTimeline(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id || !this.newTimelineTitle.trim() || !this.newTimelineDate) return;
    await this.workspaceService.createTimeline(id, this.newTimelineTitle.trim(), this.newTimelineDate, this.newTimelineDescription.trim());
    this.newTimelineTitle = ''; this.newTimelineDate = ''; this.newTimelineDescription = ''; this.appState.closeModal(); await this.loadTimeline();
  }
  async deleteTimeline(id: string): Promise<void> { await this.workspaceService.deleteTimeline(id); await this.loadTimeline(); }

  async loadPlanning(): Promise<void> { const id = this.appState.activeUniverseId(); if (id) this.planning.set(await this.workspaceService.listPlanning(id)); }
  planningByStatus(status: PlanningStatus): PlanningItem[] { return this.planning().filter((item) => item.status === status); }
  async createPlanning(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id || !this.newPlanningTitle.trim()) return;
    await this.workspaceService.createPlanning(id, this.newPlanningTitle.trim(), this.newPlanningDescription.trim());
    this.newPlanningTitle = ''; this.newPlanningDescription = ''; this.appState.closeModal(); await this.loadPlanning();
  }
  async movePlanning(item: PlanningItem, direction: -1 | 1): Promise<void> {
    const index = this.planningStatuses.indexOf(item.status) + direction; if (index < 0 || index >= this.planningStatuses.length) return;
    await this.workspaceService.movePlanning(item.id, this.planningStatuses[index]); await this.loadPlanning();
  }
  async deletePlanning(id: string): Promise<void> { await this.workspaceService.deletePlanning(id); await this.loadPlanning(); }
  async loadHistory(): Promise<void> { const id = this.appState.activeUniverseId(); if (id) this.history.set(await this.workspaceService.listHistory(id)); }

  setTheme(value: string): void { this.theme.setTheme(value as ThemePreference); }
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
  coverStyle(universe: UniverseWithStats): Record<string, string> { return universe.cover_image ? { 'background-image': `linear-gradient(180deg, transparent, rgba(17, 11, 42, .88)), url("${universe.cover_image}")` } : {}; }
  private async refreshUniverseStats(): Promise<void> {
    const id = this.appState.activeUniverseId(); if (!id) return; const stats = await this.universeService.getStats(id);
    this.universes.update((items) => items.map((item) => item.id === id ? { ...item, stats } : item));
    const active = this.appState.activeUniverse(); if (active) this.appState.activeUniverse.set({ ...active, stats });
  }
  private clearWritingSelection(): void { this.activeStory.set(null); this.activeBook.set(null); this.activeChapter.set(null); this.books.set([]); this.chapters.set([]); this.editorTitle.set(''); this.editorContent.set(''); }
  private countWords(content: string): number { const normalized = content.replace(/<[^>]+>/g, ' ').trim(); return normalized ? normalized.split(/\s+/u).length : 0; }
  private showInfo(message: string): void { this.infoMessage.set(message); if (this.infoTimer) clearTimeout(this.infoTimer); this.infoTimer = setTimeout(() => this.infoMessage.set(''), 4200); }
  private reportError(message: string, error: unknown): void { console.error(`[NarraHub] ${message}`, error); this.errorMessage.set(`${message} ${error instanceof Error ? error.message : String(error)}`); }
}
