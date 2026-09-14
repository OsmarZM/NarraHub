import { Component, HostListener, OnDestroy, ViewEncapsulation, computed, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { isTauri } from '@tauri-apps/api/core';
import { AppBootstrapService } from './bootstrap/app-bootstrap.service';
import { NativeWindowService } from './core/native/window.service';
import { SchemaRecoveryComponent } from './bootstrap/schema-recovery.component';
import { AppNavigationService } from './core/navigation/app-navigation.service';
import { AppNavigationId } from './core/navigation/app-navigation';
import { MobileNavigationComponent } from './shell/mobile-navigation/mobile-navigation.component';
import { MobileNavigationOption } from './shell/mobile-navigation/mobile-navigation.model';
import { ViewportState } from './shell/state/viewport.state';
import { AppState } from './core/state/app.state';
import { CollaborationStore } from './features/collaboration/state/collaboration.store';
import { ManuscriptStore } from './features/manuscript/state/manuscript.store';
import { SettingsStore } from './features/settings/state/settings.store';
import { AppShellComponent } from './shell/app-shell/app-shell.component';
import { ShellState } from './shell/state/shell.state';
import { TitlebarComponent } from './shell/titlebar/titlebar.component';

@Component({
  selector: 'app-root-layout',
  standalone: true,
  imports: [RouterOutlet, AppShellComponent, TitlebarComponent, SchemaRecoveryComponent, MobileNavigationComponent],
  templateUrl: './root-layout.component.html',
  styleUrl: './root-layout.component.css',
  encapsulation: ViewEncapsulation.None,
  host: { '[class.nh-mobile]': 'viewport.isMobile()' },
})
export class RootLayoutComponent implements OnDestroy {
  readonly bootstrap = inject(AppBootstrapService);
  private readonly nativeWindow = inject(NativeWindowService);
  readonly shell = inject(ShellState);
  private readonly appState = inject(AppState);
  private readonly navigation = inject(AppNavigationService);
  private readonly collaboration = inject(CollaborationStore);
  private readonly manuscript = inject(ManuscriptStore);
  private readonly settings = inject(SettingsStore);
  readonly viewport = inject(ViewportState);

  readonly workspaceMode = computed(() => this.navigation.route().universeId !== null);

  /**
   * O universo que a navegação gestual usa para destinos que precisam de um.
   *
   * O da rota primeiro; se a tela atual é global (Universos, Configurações), o último aberto —
   * é o que deixa "Configurações → Personagens" funcionar sem voltar à biblioteca.
   */
  private readonly navigationUniverseId = computed(
    () => this.navigation.route().universeId ?? this.appState.activeUniverseId(),
  );

  readonly mobileActiveId = computed(() => this.navigation.route().navId);
  readonly mobileContextLabel = computed(() => this.appState.activeUniverse()?.name ?? '');

  readonly mobileOptions = computed<MobileNavigationOption[]>(() => {
    const universe = this.navigationUniverseId();
    return this.navigation.navigationItems.map((item) => {
      const presentation = MOBILE_PRESENTATION[item.navigationId];
      const needsContext = item.needsUniverse && !universe;
      return {
        id: item.navigationId,
        label: presentation.label,
        icon: item.icon,
        description: needsContext ? 'Abra um universo primeiro' : presentation.description,
        needsContext,
      };
    });
  });
  readonly updateBusy = this.settings.updateBusy;
  readonly updatePhase = this.settings.updatePhase;
  readonly updateInfo = this.settings.updateInfo;
  readonly updateProgress = this.settings.updateProgress;
  readonly updatePromptDismissed = this.settings.updatePromptDismissed;

  ngOnDestroy(): void {
    this.shell.dispose();
  }

  @HostListener('document:keydown.control.k', ['$event'])
  focusSearch(event: Event): void {
    event.preventDefault();
    document.querySelector<HTMLInputElement>('.nh-global-search input')?.focus();
  }

  @HostListener('document:keydown.escape')
  clearSearch(): void {
    this.shell.searchQuery.set('');
  }

  async returnToLibrary(): Promise<void> {
    await this.manuscript.saveNow();
    this.appState.goHome();
    this.shell.clearWorkspaceUi();
    await this.navigation.navigate('inicio', null);
  }

  async openSettings(): Promise<void> {
    await this.manuscript.saveNow();
    this.shell.clearWorkspaceUi();
    await this.navigation.navigate('configuracoes', null);
  }

  /**
   * A escolha feita no navegador gestual.
   *
   * O mesmo caminho dos botões do desktop: salva o capítulo antes de sair e navega pelo
   * `AppNavigationService`, que respeita os guards da rota. O navegador não sabe nada disso.
   */
  async navigateFromMobile(id: string): Promise<void> {
    const navId = id as AppNavigationId;
    await this.manuscript.saveNow();
    if (navId === 'inicio') {
      this.appState.goHome();
      this.shell.clearWorkspaceUi();
      await this.navigation.navigate('inicio', null);
      return;
    }
    if (navId === 'configuracoes') this.shell.clearWorkspaceUi();
    await this.navigation.navigate(navId, navId === 'configuracoes' ? null : this.navigationUniverseId());
  }

  async minimizeWindow(): Promise<void> {
    await this.nativeWindow.minimize();
  }

  async toggleMaximizeWindow(): Promise<void> {
    await this.nativeWindow.toggleMaximize();
  }

  async closeWindow(): Promise<void> {
    await this.manuscript.saveNow();
    await this.collaboration.syncIncoming();
    await this.collaboration.endAllActiveQuietly();
    await this.collaboration.stopShareQuietly();
    if (this.nativeWindow.available) {
      this.bootstrap.shutdown();
      await this.nativeWindow.close();
    }
  }

  async installUpdate(): Promise<void> {
    await this.manuscript.saveNow();
    if (this.manuscript.saveMessage() === 'Erro ao salvar') {
      this.shell.showError('Não foi possível instalar a atualização.', new Error('A atualização foi interrompida porque o capítulo atual não pôde ser salvo.'));
      return;
    }
    const result = await this.settings.installUpdate();
    if (!result.ok) this.shell.showError('Não foi possível instalar a atualização.', new Error(result.error || ''));
  }

  dismissUpdatePrompt(): void {
    this.settings.dismissUpdatePrompt();
  }
}

/**
 * Como cada destino se apresenta no celular.
 *
 * Os ids e ícones continuam vindo das rotas; isto é só texto de cartão. "História" e
 * "Personagens" são os nomes que o escritor usa, e cabem num cartão grande onde "Escrita" e
 * "Entidades" parecem jargão de ferramenta.
 */
const MOBILE_PRESENTATION: Record<AppNavigationId, { label: string; description: string }> = {
  inicio: { label: 'Universos', description: 'Todos os seus mundos' },
  escrita: { label: 'História', description: 'Livros e capítulos' },
  entidades: { label: 'Personagens', description: 'Personagens, lugares e objetos' },
  conexoes: { label: 'Conexões', description: 'Quem se liga a quem' },
  timeline: { label: 'Timeline', description: 'Os eventos no tempo' },
  planejamento: { label: 'Planejamento', description: 'Cards e quadros' },
  historico: { label: 'Histórico', description: 'O que mudou e quando' },
  configuracoes: { label: 'Configurações', description: 'Aparelho, backup e sincronização' },
};
