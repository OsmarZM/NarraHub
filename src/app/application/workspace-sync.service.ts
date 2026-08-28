import { Injectable, inject } from '@angular/core';
import { Entity } from '../core/models';
import { AppState } from '../core/state/app.state';
import { ConnectionsStore } from '../features/connections/state/connections.store';
import { EntityMutationKind } from '../features/entities/entities-page/entities-page.component';
import { KnowledgeStore } from '../features/knowledge/state/knowledge.store';
import { UniverseStore } from '../features/library/state/universe.store';
import { ManuscriptStore } from '../features/manuscript/state/manuscript.store';

/**
 * Coordenação entre domínios que nenhuma feature isolada pode assumir sozinha:
 * excluir uma entidade mexe em conexões e menções; salvar um capítulo mexe em
 * menções e nas estatísticas do universo.
 *
 * Isto vive acima das features de propósito (camada `application/`, como
 * `bootstrap/`), e não dentro de um layout: a Fase 3 desmonta os layouts, e o
 * plano exige que essa coordenação não migre de um componente monolítico para
 * outro. Cada página roteada chama o que precisa daqui, sem conhecer as demais.
 */
@Injectable({ providedIn: 'root' })
export class WorkspaceSyncService {
  private readonly appState = inject(AppState);
  private readonly universeStore = inject(UniverseStore);
  private readonly manuscriptStore = inject(ManuscriptStore);
  private readonly connectionsStore = inject(ConnectionsStore);
  private readonly knowledgeStore = inject(KnowledgeStore);

  /** Recalcula as estatísticas exibidas no cabeçalho/biblioteca do universo ativo. */
  async refreshUniverseStats(): Promise<void> {
    const id = this.appState.activeUniverseId();
    if (!id) return;
    const updated = await this.universeStore.refreshStats(id);
    if (updated && this.appState.activeUniverseId() === id) this.appState.activeUniverse.set(updated);
  }

  /** Efeitos de criar/renomear/excluir uma entidade fora do domínio de Entidades. */
  async onEntityMutated(kind: EntityMutationKind): Promise<void> {
    const universeId = this.appState.activeUniverseId();
    if (kind === 'created') { await this.refreshUniverseStats(); return; }
    if (kind === 'updated') return;

    const tasks: Promise<unknown>[] = [];
    if (universeId) {
      tasks.push(this.connectionsStore.load(universeId));
      tasks.push(this.knowledgeStore.refreshMentionOccurrences(universeId));
    }
    if (kind === 'deleted') {
      tasks.push(this.refreshUniverseStats());
      tasks.push(this.manuscriptStore.refreshUniverseLists());
    }
    await Promise.all(tasks);
  }

  /** Efeitos de um capítulo persistido: reindexar menções e atualizar estatísticas. */
  onChapterPersisted(chapterId: string, content: string, entities: Entity[]): void {
    void this.knowledgeStore.syncChapterMentions(chapterId, content, entities);
    void this.refreshUniverseStats();
  }
}
