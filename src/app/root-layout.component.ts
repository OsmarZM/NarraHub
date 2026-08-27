import { Component, HostListener, OnDestroy, ViewEncapsulation, computed, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { AppBootstrapService } from './core/bootstrap/app-bootstrap.service';
import { AppNavigationService } from './core/navigation/app-navigation.service';
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
  imports: [RouterOutlet, AppShellComponent, TitlebarComponent],
  templateUrl: './root-layout.component.html',
  styleUrl: './root-layout.component.css',
  encapsulation: ViewEncapsulation.None,
})
export class RootLayoutComponent implements OnDestroy {
  readonly bootstrap = inject(AppBootstrapService);
  readonly shell = inject(ShellState);
  private readonly appState = inject(AppState);
  private readonly navigation = inject(AppNavigationService);
  private readonly collaboration = inject(CollaborationStore);
  private readonly manuscript = inject(ManuscriptStore);
  private readonly settings = inject(SettingsStore);

  readonly workspaceMode = computed(() => this.navigation.route().universeId !== null);
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

  async minimizeWindow(): Promise<void> {
    if (isTauri()) await getCurrentWindow().minimize();
  }

  async toggleMaximizeWindow(): Promise<void> {
    if (isTauri()) await getCurrentWindow().toggleMaximize();
  }

  async closeWindow(): Promise<void> {
    await this.manuscript.saveNow();
    await this.collaboration.syncIncoming();
    await this.collaboration.endAllActiveQuietly();
    await this.collaboration.stopShareQuietly();
    if (isTauri()) {
      this.bootstrap.shutdown();
      await getCurrentWindow().close();
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
