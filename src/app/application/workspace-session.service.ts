import { Injectable, inject, signal } from '@angular/core';
import { UniverseWithStats } from '../core/models';
import { AppState } from '../core/state/app.state';
import { ConnectionsStore } from '../features/connections/state/connections.store';
import { EntityStore } from '../features/entities/state/entity.store';
import { HistoryStore } from '../features/history/state/history.store';
import { KnowledgeStore } from '../features/knowledge/state/knowledge.store';
import { ManuscriptStore } from '../features/manuscript/state/manuscript.store';
import { PlanningStore } from '../features/planning/state/planning.store';
import { TimelineStore } from '../features/timeline/state/timeline.store';

const LAST_UNIVERSE_KEY = 'narrahub.lastUniverseId';

/** O que a abertura de um universo não conseguiu carregar, para quem chamou poder avisar. */
export interface WorkspaceSessionResult {
  /** `null` quando tudo carregou. A abertura acontece de qualquer forma. */
  preloadError: unknown | null;
}

/**
 * Dono do ciclo de vida de uma sessão de universo: abrir, fechar, trocar e esquecer.
 *
 * Existe porque isso morava no `WorkspaceLayout`, junto de navegação, busca,
 * compartilhamento, backup e atualização. O layout precisava saber **quais** stores existem e
 * em que ordem zerá-los — conhecimento que não é dele.
 *
 * O serviço não conhece rota nem interface: quem navega e quem mostra mensagem é o layout.
 * Aqui só vive o que acontece com os dados quando a sessão muda.
 */
@Injectable({ providedIn: 'root' })
export class WorkspaceSessionService {
  private readonly appState = inject(AppState);
  private readonly manuscript = inject(ManuscriptStore);
  private readonly entities = inject(EntityStore);
  private readonly knowledge = inject(KnowledgeStore);
  private readonly connections = inject(ConnectionsStore);
  private readonly timeline = inject(TimelineStore);
  private readonly history = inject(HistoryStore);
  private readonly planning = inject(PlanningStore);

  /** Sobrevive ao fechamento do app para a biblioteca destacar o último universo aberto. */
  readonly lastOpenedUniverseId = signal<string | null>(localStorage.getItem(LAST_UNIVERSE_KEY));

  /**
   * Marca a sessão atual. Uma pré-carga que termina depois de o usuário ter trocado de
   * universo pertence a uma época anterior e é descartada — sem isso, resposta lenta de um
   * universo antigo sobrescreveria o novo.
   */
  private epoch = 0;
  private loadedUniverseId: string | null = null;

  /** Abre o universo: zera o que era do anterior e carrega o que a sessão precisa. */
  async open(universe: UniverseWithStats): Promise<WorkspaceSessionResult> {
    await this.saveActiveChapter();
    this.epoch += 1;
    this.reset();
    localStorage.setItem(LAST_UNIVERSE_KEY, universe.id);
    this.lastOpenedUniverseId.set(universe.id);
    this.appState.openUniverse(universe);
    return { preloadError: await this.preload() };
  }

  /** Fecha a sessão e volta ao estado de biblioteca. */
  async close(): Promise<void> {
    await this.saveActiveChapter();
    this.epoch += 1;
    this.appState.goHome();
    this.reset();
  }

  /** Esquece um universo excluído. Devolve `true` se ele era o último aberto. */
  forget(universeId: string): boolean {
    if (this.lastOpenedUniverseId() !== universeId) return false;
    localStorage.removeItem(LAST_UNIVERSE_KEY);
    this.lastOpenedUniverseId.set(null);
    return true;
  }

  /** Zera todos os stores de domínio. A ordem não importa; a completude, sim. */
  reset(): void {
    this.loadedUniverseId = null;
    this.manuscript.reset();
    this.entities.reset();
    this.knowledge.reset();
    this.connections.reset();
    this.timeline.reset();
    this.history.reset();
    this.planning.reset();
  }

  async saveActiveChapter(): Promise<void> {
    await this.manuscript.saveNow();
  }

  /**
   * Garante que a sessão do universo está carregada, sem repetir a pré-carga se ela já
   * aconteceu para o mesmo universo.
   *
   * Existe para o caminho de restauração de rota: voltar para uma seção de um universo que já
   * está aberto não deve recarregar cinco domínios. Quem sabe se a carga já ocorreu é este
   * serviço — antes, o layout guardava esse estado e precisava lembrar de zerá-lo junto com
   * os stores.
   */
  async ensureLoaded(universeId: string): Promise<unknown | null> {
    if (this.loadedUniverseId === universeId) return null;
    return this.preload();
  }

  /**
   * Pré-carga deliberada, não preguiça: a busca global do cabeçalho é cross-domain e precisa
   * dos cinco domínios sem que o usuário tenha visitado cada seção.
   *
   * `force` porque abrir um universo é o ponto de atualização — reabrir o mesmo universo tem
   * que trazer dado fresco. Os stores têm guarda de deduplicação, então a montagem de cada
   * página logo em seguida não repete o SQL.
   *
   * Devolve o erro em vez de mostrá-lo: este serviço não conhece interface. Uma pré-carga que
   * falha **não** aborta a abertura — o universo abre, e quem chamou decide como avisar.
   */
  private async preload(): Promise<unknown | null> {
    const id = this.appState.activeUniverseId();
    if (!id) return null;
    const epoch = this.epoch;
    try {
      await Promise.all([
        this.entities.load(id, true),
        this.timeline.load(id, true),
        this.manuscript.load(id, true),
        this.planning.load(id, true),
        this.knowledge.load(id, true),
      ]);
      if (epoch !== this.epoch || this.appState.activeUniverseId() !== id) return null;
      this.loadedUniverseId = id;
      void this.knowledge.rebuildMentionIndex(
        id,
        this.manuscript.universeChapters(),
        this.entities.entities(),
      );
      return null;
    } catch (error) {
      return error;
    }
  }
}
