import { Injectable, inject } from '@angular/core';
import { CanvasEdge, CanvasEndpointKind, CanvasEntityPosition, CanvasNode, CanvasNodeKind } from '../models';
import { DatabaseService } from './database.service';

export interface CanvasNodePatch {
  text?: string;
  image?: string;
  color?: string;
}

export interface CanvasEndpoint {
  kind: CanvasEndpointKind;
  id: string;
}

/**
 * Acesso SQL ao canvas da tela de Conexões (schema v14): elementos livres,
 * posições das entidades e ligações visuais.
 *
 * As ligações são polimórficas — cada ponta pode ser uma entidade ou um
 * elemento livre —, então não há FK protegendo `source_id`/`target_id`. Duas
 * defesas substituem a FK: `listEdges` só devolve ligação cujas duas pontas
 * ainda existem, e excluir um elemento livre apaga as ligações dele.
 */
@Injectable({ providedIn: 'root' })
export class CanvasService {
  private readonly db = inject(DatabaseService);

  listNodes(universeId: string): Promise<CanvasNode[]> {
    return this.db.select<CanvasNode>(
      'SELECT * FROM canvas_nodes WHERE universe_id = $1 ORDER BY created_at',
      [universeId],
    );
  }

  listEntityPositions(universeId: string): Promise<CanvasEntityPosition[]> {
    return this.db.select<CanvasEntityPosition>(
      'SELECT entity_id, position_x, position_y FROM canvas_entity_positions WHERE universe_id = $1',
      [universeId],
    );
  }

  /** Descarta ligações órfãs: sem FK, uma ponta pode ter sido excluída por fora. */
  listEdges(universeId: string): Promise<CanvasEdge[]> {
    return this.db.select<CanvasEdge>(
      `SELECT * FROM canvas_edges
       WHERE universe_id = $1
         AND ((source_kind = 'entity' AND source_id IN (SELECT id FROM entities WHERE universe_id = $1))
           OR (source_kind = 'canvas' AND source_id IN (SELECT id FROM canvas_nodes WHERE universe_id = $1)))
         AND ((target_kind = 'entity' AND target_id IN (SELECT id FROM entities WHERE universe_id = $1))
           OR (target_kind = 'canvas' AND target_id IN (SELECT id FROM canvas_nodes WHERE universe_id = $1)))
       ORDER BY created_at`,
      [universeId],
    );
  }

  async createNode(universeId: string, kind: CanvasNodeKind, text: string, image = '', x = 0, y = 0): Promise<CanvasNode> {
    const now = this.db.now();
    const node: CanvasNode = {
      id: this.db.generateId(), universe_id: universeId, kind, text, image, color: '',
      position_x: x, position_y: y, created_at: now, updated_at: now,
    };
    await this.db.execute(
      `INSERT INTO canvas_nodes (id, universe_id, kind, text, image, color, position_x, position_y, created_at, updated_at)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)`,
      [node.id, universeId, kind, text, image, '', x, y, now],
    );
    return node;
  }

  async updateNode(id: string, patch: CanvasNodePatch): Promise<void> {
    const fields: string[] = [];
    const values: unknown[] = [];
    for (const key of ['text', 'image', 'color'] as const) {
      if (patch[key] !== undefined) { fields.push(`${key} = $${fields.length + 1}`); values.push(patch[key]); }
    }
    if (!fields.length) return;
    values.push(this.db.now(), id);
    await this.db.execute(
      `UPDATE canvas_nodes SET ${fields.join(', ')}, updated_at = $${values.length - 1} WHERE id = $${values.length}`,
      values,
    );
  }

  /** Apaga o elemento e as ligações dele — o que a FK faria se as pontas não fossem polimórficas. */
  async deleteNode(id: string): Promise<void> {
    await this.db.execute(
      `DELETE FROM canvas_edges WHERE (source_kind = 'canvas' AND source_id = $1) OR (target_kind = 'canvas' AND target_id = $1)`,
      [id],
    );
    await this.db.execute('DELETE FROM canvas_nodes WHERE id = $1', [id]);
  }

  async saveNodePosition(id: string, x: number, y: number): Promise<void> {
    await this.db.execute(
      'UPDATE canvas_nodes SET position_x = $1, position_y = $2, updated_at = $3 WHERE id = $4',
      [x, y, this.db.now(), id],
    );
  }

  async saveEntityPosition(universeId: string, entityId: string, x: number, y: number): Promise<void> {
    await this.db.execute(
      `INSERT INTO canvas_entity_positions (universe_id, entity_id, position_x, position_y, updated_at)
       VALUES ($1,$2,$3,$4,$5)
       ON CONFLICT(universe_id, entity_id) DO UPDATE SET position_x = $3, position_y = $4, updated_at = $5`,
      [universeId, entityId, x, y, this.db.now()],
    );
  }

  /** Esquece o layout salvo para o universo voltar ao arranjo automático. */
  async clearLayout(universeId: string): Promise<void> {
    await this.db.execute('DELETE FROM canvas_entity_positions WHERE universe_id = $1', [universeId]);
  }

  async createEdge(universeId: string, source: CanvasEndpoint, target: CanvasEndpoint, label: string): Promise<CanvasEdge> {
    const edge: CanvasEdge = {
      id: this.db.generateId(), universe_id: universeId,
      source_kind: source.kind, source_id: source.id,
      target_kind: target.kind, target_id: target.id,
      label, created_at: this.db.now(),
    };
    await this.db.execute(
      `INSERT INTO canvas_edges (id, universe_id, source_kind, source_id, target_kind, target_id, label, created_at)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8)`,
      [edge.id, universeId, source.kind, source.id, target.kind, target.id, label, edge.created_at],
    );
    return edge;
  }

  async deleteEdge(id: string): Promise<void> {
    await this.db.execute('DELETE FROM canvas_edges WHERE id = $1', [id]);
  }
}
