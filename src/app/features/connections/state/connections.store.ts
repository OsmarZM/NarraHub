import { Injectable, computed, inject, signal } from '@angular/core';
import { CanvasEdge, CanvasEntityPosition, CanvasNode, CanvasNodeKind, RelationCard } from '../../../core/models';
import { CanvasEndpointRef, ConnectionsGateway } from '../gateways/connections.gateway';

@Injectable({ providedIn: 'root' })
export class ConnectionsStore {
  private readonly gateway = inject(ConnectionsGateway);
  private requestedUniverseId = '';
  private loadRevision = 0;

  readonly relations = signal<RelationCard[]>([]);
  readonly canvasNodes = signal<CanvasNode[]>([]);
  readonly canvasEdges = signal<CanvasEdge[]>([]);
  readonly entityPositions = signal<CanvasEntityPosition[]>([]);
  readonly busy = signal(false);
  readonly error = signal('');

  /** Mapa id→posição, do jeito que o grafo consome ao montar os nós. */
  readonly positionByEntityId = computed(() => {
    const map: Record<string, { x: number; y: number }> = {};
    for (const item of this.entityPositions()) map[item.entity_id] = { x: item.position_x, y: item.position_y };
    return map;
  });

  readonly hasSavedLayout = computed(() => this.entityPositions().length > 0 || this.canvasNodes().length > 0);

  async load(universeId: string): Promise<void> {
    if (!universeId) { this.reset(); return; }
    const revision = ++this.loadRevision;
    this.requestedUniverseId = universeId;
    this.error.set('');
    try {
      const [relations, nodes, edges, positions] = await Promise.all([
        this.gateway.listRelations(universeId),
        this.gateway.listCanvasNodes(universeId),
        this.gateway.listCanvasEdges(universeId),
        this.gateway.listEntityPositions(universeId),
      ]);
      if (revision !== this.loadRevision || this.requestedUniverseId !== universeId) return;
      this.relations.set(relations);
      this.canvasNodes.set(nodes);
      this.canvasEdges.set(edges);
      this.entityPositions.set(positions);
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar as conexões.');
    }
  }

  reset(): void {
    this.loadRevision += 1;
    this.requestedUniverseId = '';
    this.relations.set([]);
    this.canvasNodes.set([]);
    this.canvasEdges.set([]);
    this.entityPositions.set([]);
  }

  clearError(): void { this.error.set(''); }

  // ── Relações canônicas ──────────────────────────

  async create(universeId: string, sourceId: string, targetId: string, label: string): Promise<boolean> {
    const trimmed = label.trim();
    if (!universeId || !sourceId || !targetId || !trimmed || sourceId === targetId) return false;
    return this.mutate(() => this.gateway.createRelation(universeId, sourceId, targetId, trimmed), 'Não foi possível criar a conexão.');
  }

  async delete(id: string): Promise<boolean> {
    return this.mutate(() => this.gateway.deleteRelation(id), 'Não foi possível excluir a conexão.');
  }

  // ── Elementos livres do canvas ──────────────────

  async addNode(kind: CanvasNodeKind, text: string, image = '', x = 0, y = 0): Promise<CanvasNode | null> {
    const universeId = this.requestedUniverseId;
    if (!universeId) return null;
    this.busy.set(true);
    this.error.set('');
    try {
      const node = await this.gateway.createCanvasNode(universeId, kind, text.trim(), image, x, y);
      this.canvasNodes.update((items) => [...items, node]);
      return node;
    } catch (error) {
      this.setError(error, 'Não foi possível adicionar o elemento.');
      return null;
    } finally {
      this.busy.set(false);
    }
  }

  async renameNode(id: string, text: string): Promise<boolean> {
    const trimmed = text.trim();
    try {
      await this.gateway.updateCanvasNode(id, { text: trimmed });
      this.canvasNodes.update((items) => items.map((item) => item.id === id ? { ...item, text: trimmed } : item));
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível renomear o elemento.');
      return false;
    }
  }

  async deleteNode(id: string): Promise<boolean> {
    try {
      await this.gateway.deleteCanvasNode(id);
      this.canvasNodes.update((items) => items.filter((item) => item.id !== id));
      // O gateway apaga as ligações do elemento junto; espelha isso na memória.
      this.canvasEdges.update((items) => items.filter(
        (edge) => !(edge.source_kind === 'canvas' && edge.source_id === id)
               && !(edge.target_kind === 'canvas' && edge.target_id === id),
      ));
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível excluir o elemento.');
      return false;
    }
  }

  // ── Layout ──────────────────────────────────────

  /**
   * Chamado ao soltar um nó arrastado. Falha de gravação aqui não pode
   * atrapalhar o desenho: a posição na tela já é a que o usuário vê, então o
   * erro vira só uma mensagem, sem desfazer o arrasto.
   */
  async savePosition(kind: 'entity' | 'canvas', id: string, x: number, y: number): Promise<void> {
    const universeId = this.requestedUniverseId;
    if (!universeId) return;
    try {
      if (kind === 'entity') {
        await this.gateway.saveEntityPosition(universeId, id, x, y);
        this.entityPositions.update((items) => {
          const rest = items.filter((item) => item.entity_id !== id);
          return [...rest, { entity_id: id, position_x: x, position_y: y }];
        });
      } else {
        await this.gateway.saveCanvasNodePosition(id, x, y);
        this.canvasNodes.update((items) => items.map((item) => item.id === id ? { ...item, position_x: x, position_y: y } : item));
      }
    } catch (error) {
      this.setError(error, 'A posição não pôde ser salva.');
    }
  }

  /** Esquece o layout salvo para o grafo voltar ao arranjo automático. */
  async resetLayout(): Promise<boolean> {
    const universeId = this.requestedUniverseId;
    if (!universeId) return false;
    try {
      await this.gateway.clearLayout(universeId);
      this.entityPositions.set([]);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível recomeçar o layout.');
      return false;
    }
  }

  // ── Ligações visuais ────────────────────────────

  async addEdge(source: CanvasEndpointRef, target: CanvasEndpointRef, label: string): Promise<boolean> {
    const universeId = this.requestedUniverseId;
    if (!universeId) return false;
    if (source.kind === target.kind && source.id === target.id) return false;
    try {
      const edge = await this.gateway.createCanvasEdge(universeId, source, target, label.trim());
      this.canvasEdges.update((items) => [...items, edge]);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível criar a ligação.');
      return false;
    }
  }

  async deleteEdge(id: string): Promise<boolean> {
    try {
      await this.gateway.deleteCanvasEdge(id);
      this.canvasEdges.update((items) => items.filter((item) => item.id !== id));
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível excluir a ligação.');
      return false;
    }
  }

  private async mutate(operation: () => Promise<void>, fallback: string): Promise<boolean> {
    const universeId = this.requestedUniverseId;
    this.busy.set(true);
    this.error.set('');
    try {
      await operation();
      await this.load(universeId);
      return true;
    } catch (error) {
      this.setError(error, fallback);
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Connections operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
