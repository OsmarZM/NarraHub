import { Injectable, inject } from '@angular/core';
import { CanvasEdge, CanvasEntityPosition, CanvasNode, CanvasNodeKind, RelationCard } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { CanvasEndpointRef, CanvasNodePatchInput, ConnectionsGateway } from './connections.gateway';

/**
 * Domínio migrado por inteiro. Ordem 1 — `listRelations`. Ordem 5 —
 * `createRelation`, `deleteRelation`. Ordem 10 — o canvas.
 *
 * `relations` e `canvas_edges` continuam separados no core, como a Fase 3
 * decidiu: relação é fato do universo e aparece na ficha; ligação de canvas é
 * anotação de diagrama, com pontas polimórficas.
 */
@Injectable({ providedIn: 'root' })
export class RustConnectionsGateway implements ConnectionsGateway {
  private readonly core = inject(RustCoreService);

  listRelations(universeId: string): Promise<RelationCard[]> {
    return this.core.call<RelationCard[]>('relations_list', { universeId });
  }

  async createRelation(universeId: string, sourceId: string, targetId: string, label: string): Promise<void> {
    await this.core.call<string>('relation_create', { universeId, sourceId, targetId, label });
  }

  deleteRelation(id: string): Promise<void> {
    return this.core.call<void>('relation_delete', { id });
  }

  listCanvasNodes(universeId: string): Promise<CanvasNode[]> {
    return this.core.call<CanvasNode[]>('canvas_nodes', { universeId });
  }

  createCanvasNode(universeId: string, kind: CanvasNodeKind, text: string, image: string, x: number, y: number): Promise<CanvasNode> {
    return this.core.call<CanvasNode>('canvas_node_create', { universeId, kind, text, image, x, y });
  }

  updateCanvasNode(id: string, patch: CanvasNodePatchInput): Promise<void> {
    return this.core.call<void>('canvas_node_update', { id, patch });
  }

  deleteCanvasNode(id: string): Promise<void> {
    return this.core.call<void>('canvas_node_delete', { id });
  }

  saveCanvasNodePosition(id: string, x: number, y: number): Promise<void> {
    return this.core.call<void>('canvas_node_position', { id, x, y });
  }

  listEntityPositions(universeId: string): Promise<CanvasEntityPosition[]> {
    return this.core.call<CanvasEntityPosition[]>('canvas_entity_positions', { universeId });
  }

  saveEntityPosition(universeId: string, entityId: string, x: number, y: number): Promise<void> {
    return this.core.call<void>('canvas_entity_position_save', { universeId, entityId, x, y });
  }

  clearLayout(universeId: string): Promise<void> {
    return this.core.call<void>('canvas_layout_clear', { universeId });
  }

  listCanvasEdges(universeId: string): Promise<CanvasEdge[]> {
    return this.core.call<CanvasEdge[]>('canvas_edges', { universeId });
  }

  createCanvasEdge(universeId: string, source: CanvasEndpointRef, target: CanvasEndpointRef, label: string): Promise<CanvasEdge> {
    return this.core.call<CanvasEdge>('canvas_edge_create', { universeId, source, target, label });
  }

  deleteCanvasEdge(id: string): Promise<void> {
    return this.core.call<void>('canvas_edge_delete', { id });
  }
}
