import { Injectable, inject } from '@angular/core';
import { CanvasEdge, CanvasEntityPosition, CanvasNode, CanvasNodeKind, RelationCard } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { CanvasEndpointRef, CanvasNodePatchInput, ConnectionsGateway } from './connections.gateway';
import { LegacyConnectionsGateway } from './legacy-connections.gateway';

/**
 * Ordem 1 (leitura) — migrado: `listRelations`.
 * Ordem 5 (relações e menções) — migrado: `createRelation`, `deleteRelation`.
 *
 * O canvas continua no legado de propósito: ele não está na ordem de migração
 * do plano, é anotação de diagrama e não fato do universo, e migrá-lo junto
 * misturaria duas coisas que a Fase 3 separou.
 */
@Injectable({ providedIn: 'root' })
export class RustConnectionsGateway implements ConnectionsGateway {
  private readonly core = inject(RustCoreService);
  private readonly legacy = inject(LegacyConnectionsGateway);

  listRelations(universeId: string): Promise<RelationCard[]> {
    return this.core.call<RelationCard[]>('relations_list', { universeId });
  }

  createRelation(universeId: string, sourceId: string, targetId: string, label: string): Promise<void> {
    return this.legacy.createRelation(universeId, sourceId, targetId, label);
  }

  deleteRelation(id: string): Promise<void> {
    return this.legacy.deleteRelation(id);
  }

  listCanvasNodes(universeId: string): Promise<CanvasNode[]> {
    return this.legacy.listCanvasNodes(universeId);
  }

  createCanvasNode(universeId: string, kind: CanvasNodeKind, text: string, image: string, x: number, y: number): Promise<CanvasNode> {
    return this.legacy.createCanvasNode(universeId, kind, text, image, x, y);
  }

  updateCanvasNode(id: string, patch: CanvasNodePatchInput): Promise<void> {
    return this.legacy.updateCanvasNode(id, patch);
  }

  deleteCanvasNode(id: string): Promise<void> {
    return this.legacy.deleteCanvasNode(id);
  }

  saveCanvasNodePosition(id: string, x: number, y: number): Promise<void> {
    return this.legacy.saveCanvasNodePosition(id, x, y);
  }

  listEntityPositions(universeId: string): Promise<CanvasEntityPosition[]> {
    return this.legacy.listEntityPositions(universeId);
  }

  saveEntityPosition(universeId: string, entityId: string, x: number, y: number): Promise<void> {
    return this.legacy.saveEntityPosition(universeId, entityId, x, y);
  }

  clearLayout(universeId: string): Promise<void> {
    return this.legacy.clearLayout(universeId);
  }

  listCanvasEdges(universeId: string): Promise<CanvasEdge[]> {
    return this.legacy.listCanvasEdges(universeId);
  }

  createCanvasEdge(universeId: string, source: CanvasEndpointRef, target: CanvasEndpointRef, label: string): Promise<CanvasEdge> {
    return this.legacy.createCanvasEdge(universeId, source, target, label);
  }

  deleteCanvasEdge(id: string): Promise<void> {
    return this.legacy.deleteCanvasEdge(id);
  }
}
