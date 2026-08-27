import { Injectable, inject, signal } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';
import { AiService } from '../core/services/ai.service';
import { DatabaseService } from '../core/services/database.service';
import { CollaborationStore } from '../features/collaboration/state/collaboration.store';
import { KnowledgeStore } from '../features/knowledge/state/knowledge.store';
import { UniverseStore } from '../features/library/state/universe.store';
import { SettingsStore } from '../features/settings/state/settings.store';

@Injectable({ providedIn: 'root' })
export class AppBootstrapService {
  private readonly ai = inject(AiService);
  private readonly db = inject(DatabaseService);
  private readonly collaboration = inject(CollaborationStore);
  private readonly knowledge = inject(KnowledgeStore);
  private readonly universes = inject(UniverseStore);
  private readonly settings = inject(SettingsStore);

  readonly ready = signal(false);
  readonly error = signal('');

  private initialization: Promise<void> | null = null;
  private collaborationTimer: ReturnType<typeof setInterval> | null = null;
  private updateTimer: ReturnType<typeof setTimeout> | null = null;

  initialize(): Promise<void> {
    this.initialization ??= this.runInitialization();
    return this.initialization;
  }

  shutdown(): void {
    if (this.collaborationTimer) clearInterval(this.collaborationTimer);
    if (this.updateTimer) clearTimeout(this.updateTimer);
    this.collaborationTimer = null;
    this.updateTimer = null;
    this.settings.dispose();
    this.ai.dispose();
  }

  private async runInitialization(): Promise<void> {
    this.error.set('');
    try {
      await this.ai.initialize().catch((error) => {
        console.error('[NarraHub] Não foi possível inicializar o gerenciador da IA local.', error);
      });
      if (!isTauri()) return;

      await this.db.init();
      await this.universes.load();
      await this.knowledge.refreshLibraryPreviewTags();
      await this.collaboration.refreshShareStatus();
      await this.collaboration.loadReview();
      this.collaborationTimer = setInterval(() => void this.collaboration.syncIncoming(), 2500);
      await this.settings.primeCurrentVersion();
      if (await this.settings.isUpdateConfigured()) {
        this.updateTimer = setTimeout(() => void this.settings.checkForUpdates(true), 1800);
      }
    } catch (error) {
      console.error('[NarraHub] Não foi possível inicializar a aplicação.', error);
      this.error.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.ready.set(true);
    }
  }
}
