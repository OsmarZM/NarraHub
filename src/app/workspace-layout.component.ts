import { CommonModule } from '@angular/common';
import { Component, OnDestroy, ViewChild, computed, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterOutlet } from '@angular/router';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  BookOption, ChapterOption, Entity, MetadataOwnerType, PlanningItem, UniverseWithStats,
} from './core/models';
import { BackupManifest } from './core/services/backup.service';
import { AppNavigationId, AppRouteState } from './core/navigation/app-navigation';
import { AppNavigationService } from './core/navigation/app-navigation.service';
import { AppState } from './core/state/app.state';
import { ShareCreateRequest, ShareModalComponent } from './features/collaboration/share-modal/share-modal.component';
import { CollaborationStore } from './features/collaboration/state/collaboration.store';
import { ConnectionsStore } from './features/connections/state/connections.store';
import { EntityHubType, EntityStore } from './features/entities/state/entity.store';
import { PlanningStore } from './features/planning/state/planning.store';
import { HistoryStore } from './features/history/state/history.store';
import { KnowledgeStore } from './features/knowledge/state/knowledge.store';
import { TagsModalComponent } from './features/knowledge/tags-modal/tags-modal.component';
import { UniverseStore } from './features/library/state/universe.store';
import { ManuscriptStore } from './features/manuscript/state/manuscript.store';
import { SettingsStore } from './features/settings/state/settings.store';
import { TimelineStore } from './features/timeline/state/timeline.store';
import { GlobalSearchResult, GlobalSearchService } from './application/global-search.service';
import { WorkspaceSessionService } from './application/workspace-session.service';
import { WorkspaceShareService } from './application/workspace-share.service';
import { WorkspaceSyncService } from './application/workspace-sync.service';
import { ShellState } from './shell/state/shell.state';
import { SidebarNavItem, UniverseSidebarComponent } from './shell/universe-sidebar/universe-sidebar.component';

/**
 * Contrato mínimo para uma página roteada (filha de /workspace/:universeId)
 * expor uma ação de criação disparada pelo cabeçalho persistente do
 * WorkspaceLayout — que não é mais pai de view da página no sentido do
 * Angular (ela chega pelo <router-outlet>), então @ViewChild não alcança
 * mais. O (activate) do outlet entrega a instância ativa; este type guard
 * evita `any` espalhado pelas próximas fatias que migrarem para rota.
 */
interface CreatableRoutedPage {
  openCreate(): void;
}

function supportsCreate(page: unknown): page is CreatableRoutedPage {
  return !!page && typeof (page as CreatableRoutedPage).openCreate === 'function';
}

@Component({
  selector: 'app-workspace-layout',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    RouterOutlet,
    UniverseSidebarComponent,
    ShareModalComponent,
    TagsModalComponent,
  ],
  templateUrl: './workspace-layout.component.html',
  styleUrl: './workspace-layout.component.css',
})
export class WorkspaceLayoutComponent implements OnDestroy {
  readonly Math = Math;
  readonly appState = inject(AppState);
  readonly shell = inject(ShellState);
  readonly knowledgeStore = inject(KnowledgeStore);
  private readonly universeStore = inject(UniverseStore);
  private readonly collaborationStore = inject(CollaborationStore);
  private readonly connectionsStore = inject(ConnectionsStore);
  private readonly entityStore = inject(EntityStore);
  private readonly manuscriptStore = inject(ManuscriptStore);
  private readonly planningStore = inject(PlanningStore);
  private readonly settingsStore = inject(SettingsStore);
  private readonly historyStore = inject(HistoryStore);
  private readonly timelineStore = inject(TimelineStore);
  private readonly navigation = inject(AppNavigationService);
  private readonly globalSearch = inject(GlobalSearchService);
  private readonly session = inject(WorkspaceSessionService);
  private readonly share = inject(WorkspaceShareService);
  private readonly workspaceSync = inject(WorkspaceSyncService);

  readonly searchQuery = this.shell.searchQuery;
  readonly activeNav = computed(() => this.navigation.activeData().navigationId);
  readonly universes = this.universeStore.universes;
  readonly entityFilter = this.entityStore.filter;
  readonly activeStory = this.manuscriptStore.activeStory;
  readonly activeBook = this.manuscriptStore.activeBook;
  readonly activeChapter = this.manuscriptStore.activeChapter;
  readonly saveMessage = this.manuscriptStore.saveMessage;
  readonly isSaving = this.manuscriptStore.isSaving;
  readonly inspectorOpen = this.manuscriptStore.inspectorOpen;
  readonly isFocusMode = this.shell.focusMode;
  readonly errorMessage = this.shell.errorMessage;
  readonly infoMessage = this.shell.infoMessage;
  readonly updateBusy = this.settingsStore.updateBusy;
  readonly updatePhase = this.settingsStore.updatePhase;
  readonly updateInfo = this.settingsStore.updateInfo;
  readonly updateProgress = this.settingsStore.updateProgress;
  readonly updatePromptDismissed = this.settingsStore.updatePromptDismissed;
  /** Quem guarda isto é o `WorkspaceSessionService`; aqui é só leitura para o template. */
  readonly lastOpenedUniverseId = this.session.lastOpenedUniverseId;
  readonly renamingUniverse = signal(false);

  renameValue = '';

  readonly navItems: SidebarNavItem[] = this.navigation.navigationItems.map((item) => ({
    id: item.navigationId,
    label: item.sidebarLabel ?? item.label,
    icon: item.icon,
    needsUniverse: item.needsUniverse,
  }));
  readonly libraryBreadcrumbLabel = this.navigation.navigationItems
    .find((item) => item.navigationId === 'inicio')?.label ?? 'Universos';

  readonly globalSearchResults = this.globalSearch.results;


  private restoringRoute = false;
  /**
   * Chave da última navegação já refletida em AppState.workspaceView() (e nos
   * demais efeitos colaterais de selectNav()/returnToLibrary()/openSettings()).
   * `activeNav()` sozinho não serve mais pra isso: como ele é derivado direto
   * de route.data, ele já reflete a rota nova assim que o Router resolve,
   * ANTES de qualquer um desses métodos rodar — comparar `route.navId ===
   * activeNav()` sempre dava "já processado" e restoreRoute() nunca corrigia
   * o workspaceView() depois de um deep link/reload direto pra uma seção
   * roteada (Histórico, Timeline), deixando a view antiga (ex.: 'editor')
   * grudada por baixo do <router-outlet>.
   */
  private lastSyncedRouteKey = '';
  private activeRoutedPage: unknown = null;

  constructor() {
    effect(() => {
      const route = this.navigation.route();
      void this.restoreRoute(route);
    });
  }

  ngOnDestroy(): void {
    this.session.reset();
  }
  async toggleFullscreen(): Promise<void> { if (isTauri()) { const win = getCurrentWindow(); await win.setFullscreen(!(await win.isFullscreen())); } }
  async loadUniverses(): Promise<void> {
    await this.universeStore.load();
    await this.knowledgeStore.refreshLibraryPreviewTags();
  }

  async selectNav(item: SidebarNavItem, updateRoute = true): Promise<void> {
    if (item.needsUniverse && !this.appState.activeUniverse()) {
      this.showInfo('Selecione ou crie um universo para abrir esta área.'); return;
    }
    const targetUniverseId = (item.id === 'inicio' || item.id === 'configuracoes') ? null : this.appState.activeUniverseId();
    this.lastSyncedRouteKey = this.routeKey(item.id, targetUniverseId);
    await this.saveChapterNow();
    if (item.id === 'inicio') { await this.returnToLibrary(updateRoute); return; }
    if (item.id === 'ajuda') { this.showInfo('Ajuda e feedback serão conectados ao fluxo nativo em uma próxima fase.'); return; }
    if (item.id === 'configuracoes') { this.openSettings(updateRoute); return; }
    // Entrar em Entidades pelo menu sempre mostra o hub inteiro, não o último filtro.
    if (item.id === 'entidades') this.entityStore.setFilter(null);
    // Cada página roteada carrega o próprio domínio ao montar; o layout só navega.
    if (updateRoute) await this.navigation.navigate(item.id as AppNavigationId, this.appState.activeUniverseId());
  }

  async openUniverse(universe: UniverseWithStats, updateRoute = true): Promise<void> {
    this.searchQuery.set('');
    const { preloadError } = await this.session.open(universe);
    if (preloadError) this.reportError('Não foi possível carregar os dados do universo.', preloadError);
    if (updateRoute) await this.navigation.navigate('escrita', universe.id);
  }

  onUniverseUpdated(): void { this.showInfo('Universo atualizado.'); }

  onUniverseDeleted(universeId: string): void {
    this.session.forget(universeId);
    if (this.appState.activeUniverseId() === universeId) void this.returnToLibrary();
    this.showInfo('Universo excluído do banco local.');
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
      // Só o nome entra na atualização otimista. O `updated_at` quem decide é
      // o core, e carimbar um valor local aqui exibiria uma data que não é a
      // que ficou gravada.
      this.appState.activeUniverse.update((item) => item?.id === universe.id ? { ...item, name } : item);
      await this.universeStore.update(universe.id, { name });
      this.renamingUniverse.set(false); this.renameValue = ''; this.appState.closeModal();
      this.showInfo('Universo renomeado.');
    } catch (error) { this.reportError(`Não foi possível renomear ${universe.name}.`, error); }
  }

  async returnToLibrary(updateRoute = true): Promise<void> {
    this.lastSyncedRouteKey = this.routeKey('inicio', null);
    this.searchQuery.set('');
    await this.session.close();
    if (updateRoute) await this.navigation.navigate('inicio', null);
  }

  async openSettings(updateRoute = true): Promise<void> {
    this.lastSyncedRouteKey = this.routeKey('configuracoes', null);
    await this.saveChapterNow();
    this.searchQuery.set('');
    if (updateRoute) await this.navigation.navigate('configuracoes', null);
  }

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

  toggleInspector(): void { this.manuscriptStore.toggleInspector(); }

  /**
   * Mantido porque o template e o caminho de restauração de rota chamam por aqui. A carga em
   * si é do `WorkspaceSessionService`: o layout não sabe mais quais domínios existem nem em
   * que ordem carregá-los.
   */
  async loadWorkspaceData(): Promise<void> {
    const id = this.appState.activeUniverseId();
    if (!id) return;
    const error = await this.session.ensureLoaded(id);
    if (error) this.reportError('Não foi possível carregar os dados do universo.', error);
  }

  async openBookOption(book: BookOption): Promise<void> {
    if (await this.manuscriptStore.openBookOption(book)) this.setWorkspaceNavigation('escrita');
  }

  async openChapterOption(option: ChapterOption): Promise<void> {
    if (await this.manuscriptStore.openChapterOption(option)) this.setWorkspaceNavigation('escrita');
  }

  private async saveChapterNow(): Promise<void> { await this.session.saveActiveChapter(); }


  selectEntityTab(type: EntityHubType | null): void {
    this.entityStore.setFilter(type);
  }

  entityCreateLabel(): string { return this.entityStore.createLabel(); }

  async openEntitySheet(entity: Entity): Promise<void> {
    const universeId = this.appState.activeUniverseId();
    if (!universeId || !await this.entityStore.open(universeId, entity)) {
      this.reportError('Não foi possível abrir a ficha.', this.entityStore.error());
    }
  }

  onKnowledgeFailed(message: string): void { this.reportError(message, message); }

  async openGlobalSearchResult(result: GlobalSearchResult): Promise<void> {
    this.searchQuery.set('');
    if (result.kind === 'story') {
      const opened = await this.manuscriptStore.openStoryById(result.id);
      if (opened) this.setWorkspaceNavigation('escrita');
    } else if (result.kind === 'book') {
      const book = this.manuscriptStore.universeBooks().find((item) => item.id === result.id); if (book) await this.openBookOption(book);
    } else if (result.kind === 'chapter') {
      const chapter = this.manuscriptStore.universeChapters().find((item) => item.id === result.id); if (chapter) await this.openChapterOption(chapter);
    } else if (result.kind === 'entity') {
      const entity = this.entityStore.entities().find((item) => item.id === result.id); if (entity) { this.setWorkspaceNavigation('entidades'); this.selectEntityTab(entity.type as EntityHubType); await this.openEntitySheet(entity); }
    } else if (result.kind === 'timeline') {
      this.setWorkspaceNavigation('timeline'); queueMicrotask(() => document.querySelector<HTMLElement>(`[data-timeline-id="${result.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' }));
    } else {
      this.setWorkspaceNavigation('planejamento'); queueMicrotask(() => document.querySelector<HTMLElement>(`[data-planning-id="${result.id}"]`)?.scrollIntoView({ behavior: 'smooth', block: 'center' }));
    }
  }

  /**
   * Trata o próprio erro de propósito: selectNav() dá await nisso ANTES de
   * chamar navigation.navigate(), então uma rejeição aqui abortava a troca de
   * seção inteira — a URL ficava na seção anterior enquanto workspaceView() já
   * tinha mudado, deixando a página roteada anterior e a nova página legacy
   * renderizadas ao mesmo tempo. Carregar dados de um domínio nunca deve
   * impedir a navegação.
   */
  /** Gatilho único: a página ativa chega pelo (activate) do outlet, não por @ViewChild. */
  beginCreateOnActivePage(): void {
    if (supportsCreate(this.activeRoutedPage)) this.activeRoutedPage.openCreate();
  }

  onWorkspaceOutletActivate(component: unknown): void { this.activeRoutedPage = component; }
  onWorkspaceOutletDeactivate(): void { this.activeRoutedPage = null; }

  async openMetadata(type: MetadataOwnerType, id: string, name: string, event?: Event): Promise<void> {
    event?.stopPropagation();
    await this.knowledgeStore.openMetadata(type, id, name, this.appState.activeUniverseId());
    this.appState.openModal('metadata');
  }

  closeMetadata(): void {
    this.knowledgeStore.closeMetadata();
    this.appState.closeModal();
  }

  onSettingsInfo(message: string): void { this.showInfo(message); }

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
      const document = await this.share.buildDocument({
        universes: selectedUniverses,
        includeChapters: request.includeChapters,
        includeEntities: request.includeEntities,
        permission: request.permission,
      });
      const result = await this.collaborationStore.createShare(document, request.expiresInDays, selectedUniverses.map((item) => item.id));
      if (!result.ok) { this.reportError('Não foi possível criar o compartilhamento online.', new Error(result.error || '')); return; }
      const copied = await this.copyToClipboard(this.collaborationStore.shareLink());
      this.showInfo(copied ? 'Link criptografado criado e copiado.' : 'Link criptografado criado. Copie-o manualmente.');
    } catch (error) { this.reportError('Não foi possível criar o compartilhamento online.', error); }
    finally { this.collaborationStore.shareBusy.set(false); this.collaborationStore.shareProgressMessage.set(''); }
  }

  onCollaborationApplied(universeId: string): void {
    void this.workspaceSync.onCollaborationReviewApplied(universeId);
  }

  private async copyToClipboard(text: string): Promise<boolean> {
    if (!text) return false;
    try { await navigator.clipboard.writeText(text); return true; }
    catch (error) { console.warn('[NarraHub] Não foi possível copiar o link automaticamente.', error); return false; }
  }

  formatDate(value: string): string { if (!value) return 'Sem data'; const date = new Date(value.length === 10 ? `${value}T12:00:00` : value); return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' }); }



  setWorkspaceNavigation(navId: Exclude<AppNavigationId, 'inicio' | 'configuracoes'>): void {
    // Todo call site já chama (ou está prestes a chamar) o appState.openXxx()
    // correspondente na mesma expressão — marcar a chave aqui evita que o
    // effect de restoreRoute() reprocesse a mesma navegação.
    this.lastSyncedRouteKey = this.routeKey(navId, this.appState.activeUniverseId());
    void this.navigation.navigate(navId, this.appState.activeUniverseId());
  }

  private routeKey(navId: string, universeId: string | null): string {
    return `${navId}:${universeId ?? ''}`;
  }

  private async restoreRoute(route: AppRouteState): Promise<void> {
    if (this.restoringRoute) return;
    const isRootLevel = route.navId === 'inicio' || route.navId === 'configuracoes';
    const key = this.routeKey(route.navId, isRootLevel ? null : route.universeId);
    if (key === this.lastSyncedRouteKey) return;

    this.restoringRoute = true;
    this.lastSyncedRouteKey = key;
    try {
      if (route.navId === 'inicio') {
        await this.returnToLibrary(false);
        return;
      }
      if (route.navId === 'configuracoes') {
        this.openSettings(false);
        return;
      }

      if (!route.universeId || this.appState.activeUniverseId() !== route.universeId) return;
      await this.loadWorkspaceData();
      const navItem = this.navItems.find((item) => item.id === route.navId);
      if (navItem) await this.selectNav(navItem, false);
    } finally {
      this.restoringRoute = false;
    }
  }


  private showInfo(message: string): void { this.shell.showInfo(message); }
  private reportError(message: string, error: unknown): void { this.shell.showError(message, error); }
}
