import { Injectable, inject } from '@angular/core';
import { CanvasEdge, CanvasEntityPosition, CanvasNode, CanvasNodeKind, RelationCard } from '../../../core/models';
import { CanvasService } from '../../../core/services/canvas.service';
import { WorkspaceService } from '../../../core/services/workspace.service';
import { CanvasEndpointRef, CanvasNodePatchInput, ConnectionsGateway } from './connections.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyConnectionsGateway implements ConnectionsGateway {
  private readonly workspaceService = inject(WorkspaceService);
  private readonly canvasService = inject(CanvasService);

  listRelations(universeId: string): Promise<RelationCard[]> {
    return this.workspaceService.listRelations(universeId);
  }

  createRelation(universeId: string, sourceId: string, targetId: string, label: string): Promise<void> {
    return this.workspaceService.createRelation(universeId, sourceId, targetId, label);
  }

  deleteRelation(id: string): Promise<void> {
    return this.workspaceService.deleteRelation(id);
  }

  listCanvasNodes(universeId: string): Promise<CanvasNode[]> {
    return this.canvasService.listNodes(universeId);
  }

  createCanvasNode(universeId: string, kind: CanvasNodeKind, text: string, image: string, x: number, y: number): Promise<CanvasNode> {
    return this.canvasService.createNode(universeId, kind, text, image, x, y);
  }

  updateCanvasNode(id: string, patch: CanvasNodePatchInput): Promise<void> {
    return this.canvasService.updateNode(id, patch);
  }

  deleteCanvasNode(id: string): Promise<void> {
    return this.canvasService.deleteNode(id);
  }

  saveCanvasNodePosition(id: string, x: number, y: number): Promise<void> {
    return this.canvasService.saveNodePosition(id, x, y);
  }

  listEntityPositions(universeId: string): Promise<CanvasEntityPosition[]> {
    return this.canvasService.listEntityPositions(universeId);
  }

  saveEntityPosition(universeId: string, entityId: string, x: number, y: number): Promise<void> {
    return this.canvasService.saveEntityPosition(universeId, entityId, x, y);
  }

  clearLayout(universeId: string): Promise<void> {
    return this.canvasService.clearLayout(universeId);
  }

  listCanvasEdges(universeId: string): Promise<CanvasEdge[]> {
    return this.canvasService.listEdges(universeId);
  }

  createCanvasEdge(universeId: string, source: CanvasEndpointRef, target: CanvasEndpointRef, label: string): Promise<CanvasEdge> {
    return this.canvasService.createEdge(universeId, source, target, label);
  }

  deleteCanvasEdge(id: string): Promise<void> {
    return this.canvasService.deleteEdge(id);
  }
}
