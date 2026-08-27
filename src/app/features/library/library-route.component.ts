import { Component, inject, signal } from '@angular/core';
import { UniverseWithStats } from '../../core/models';
import { AppNavigationService } from '../../core/navigation/app-navigation.service';
import { AppState } from '../../core/state/app.state';
import { ShellState } from '../../shell/state/shell.state';
import { KnowledgeStore } from '../knowledge/state/knowledge.store';
import { ManuscriptStore } from '../manuscript/state/manuscript.store';
import { LibraryPageComponent } from './library-page.component';
import { UniverseStore } from './state/universe.store';

@Component({
  selector: 'app-library-route',
  standalone: true,
  imports: [LibraryPageComponent],
  template: `
    <app-library-page
      [query]="shell.searchQuery()"
      [lastOpenedId]="lastOpenedUniverseId()"
      [tagsByUniverse]="knowledge.libraryPreviewTags()"
      (opened)="openUniverse($event)"
      (updated)="shell.showInfo('Universo atualizado.')"
      (deleted)="onUniverseDeleted($event)"
    />
  `,
  styles: [':host { display:block; width:100%; height:100%; min-height:0; }'],
})
export class LibraryRouteComponent {
  readonly shell = inject(ShellState);
  readonly knowledge = inject(KnowledgeStore);
  readonly lastOpenedUniverseId = signal<string | null>(localStorage.getItem('narrahub.lastUniverseId'));

  private readonly appState = inject(AppState);
  private readonly navigation = inject(AppNavigationService);
  private readonly manuscript = inject(ManuscriptStore);
  private readonly universes = inject(UniverseStore);

  async openUniverse(universe: UniverseWithStats): Promise<void> {
    await this.manuscript.saveNow();
    localStorage.setItem('narrahub.lastUniverseId', universe.id);
    this.lastOpenedUniverseId.set(universe.id);
    this.shell.clearWorkspaceUi();
    this.appState.openUniverse(universe);
    await this.navigation.navigate('escrita', universe.id);
  }

  async onUniverseDeleted(universeId: string): Promise<void> {
    if (this.lastOpenedUniverseId() === universeId) {
      localStorage.removeItem('narrahub.lastUniverseId');
      this.lastOpenedUniverseId.set(null);
    }
    if (this.appState.activeUniverseId() === universeId) this.appState.goHome();
    await this.universes.load();
    await this.knowledge.refreshLibraryPreviewTags();
    this.shell.showInfo('Universo excluído do banco local.');
  }
}
