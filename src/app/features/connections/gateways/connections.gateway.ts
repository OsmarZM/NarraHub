import { CanvasEdge, CanvasEntityPosition, CanvasNode, CanvasNodeKind, RelationCard } from '../../../core/models';

/** Uma ponta de ligação do canvas: entidade cadastrada ou elemento livre. */
export interface CanvasEndpointRef {
  kind: 'entity' | 'canvas';
  id: string;
}

export interface CanvasNodePatchInput {
  text?: string;
  image?: string;
  color?: string;
}

export abstract class ConnectionsGateway {
  // ── Relações canônicas (entidade ↔ entidade, aparecem na ficha) ──
  abstract listRelations(universeId: string): Promise<RelationCard[]>;
  abstract createRelation(universeId: string, sourceId: string, targetId: string, label: string): Promise<void>;
  abstract deleteRelation(id: string): Promise<void>;

  // ── Canvas: elementos livres, layout e ligações visuais ──
  abstract listCanvasNodes(universeId: string): Promise<CanvasNode[]>;
  abstract createCanvasNode(universeId: string, kind: CanvasNodeKind, text: string, image: string, x: number, y: number): Promise<CanvasNode>;
  abstract updateCanvasNode(id: string, patch: CanvasNodePatchInput): Promise<void>;
  abstract deleteCanvasNode(id: string): Promise<void>;
  abstract saveCanvasNodePosition(id: string, x: number, y: number): Promise<void>;

  abstract listEntityPositions(universeId: string): Promise<CanvasEntityPosition[]>;
  abstract saveEntityPosition(universeId: string, entityId: string, x: number, y: number): Promise<void>;
  abstract clearLayout(universeId: string): Promise<void>;

  abstract listCanvasEdges(universeId: string): Promise<CanvasEdge[]>;
  abstract createCanvasEdge(universeId: string, source: CanvasEndpointRef, target: CanvasEndpointRef, label: string): Promise<CanvasEdge>;
  abstract deleteCanvasEdge(id: string): Promise<void>;
}
