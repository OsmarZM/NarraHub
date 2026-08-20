// ============================================
// NarraHub — Relation Service
// ============================================

import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { Relation, RelationImportance, Entity, GraphNode, GraphEdge } from '../models';

@Injectable({ providedIn: 'root' })
export class RelationService {
  private db = inject(DatabaseService);

  async create(
    universeId: string,
    sourceId: string,
    targetId: string,
    label: string,
    bidirectional: boolean = false,
    importance: RelationImportance = 'normal',
    type: string = 'custom'
  ): Promise<Relation> {
    const id = this.db.generateId();
    const now = this.db.now();

    await this.db.execute(
      `INSERT INTO relations (id, universe_id, source_id, target_id, type, label, bidirectional, importance, created_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)`,
      [id, universeId, sourceId, targetId, type, label, bidirectional ? 1 : 0, importance, now]
    );

    return {
      id, universe_id: universeId, source_id: sourceId, target_id: targetId,
      type, label, bidirectional, importance, created_at: now,
    };
  }

  async listByUniverse(universeId: string): Promise<Relation[]> {
    const raw = await this.db.select<any>(
      'SELECT * FROM relations WHERE universe_id = $1',
      [universeId]
    );
    return raw.map(r => ({ ...r, bidirectional: !!r.bidirectional }));
  }

  async listByEntity(entityId: string): Promise<Relation[]> {
    const raw = await this.db.select<any>(
      'SELECT * FROM relations WHERE source_id = $1 OR target_id = $1',
      [entityId]
    );
    return raw.map(r => ({ ...r, bidirectional: !!r.bidirectional }));
  }

  async delete(id: string): Promise<void> {
    await this.db.execute('DELETE FROM relations WHERE id = $1', [id]);
  }

  async update(id: string, label: string, bidirectional: boolean, importance: RelationImportance): Promise<void> {
    await this.db.execute(
      'UPDATE relations SET label = $1, bidirectional = $2, importance = $3 WHERE id = $4',
      [label, bidirectional ? 1 : 0, importance, id]
    );
  }

  // ── Graph Data ────────────────────────────────

  async getGraphData(universeId: string): Promise<{ nodes: GraphNode[]; edges: GraphEdge[] }> {
    const entities = await this.db.select<Entity>(
      'SELECT * FROM entities WHERE universe_id = $1',
      [universeId]
    );

    const relations = await this.listByUniverse(universeId);

    const nodes: GraphNode[] = entities.map(e => ({
      id: e.id,
      name: e.name,
      type: e.type,
      image: e.image,
      canon_status: e.canon_status,
      description: e.description,
    }));

    const edges: GraphEdge[] = relations.map(r => ({
      id: r.id,
      source: r.source_id,
      target: r.target_id,
      label: r.label,
      bidirectional: r.bidirectional,
      importance: r.importance,
    }));

    return { nodes, edges };
  }

  // Get filtered graph data centered on a focal entity with depth limit
  async getFilteredGraphData(
    universeId: string,
    focalEntityId: string | null,
    depth: number,
    entityTypes: Set<string>
  ): Promise<{ nodes: GraphNode[]; edges: GraphEdge[] }> {
    const fullData = await this.getGraphData(universeId);

    // Filter by entity types
    let filteredNodes = fullData.nodes.filter(n => entityTypes.has(n.type));
    let filteredNodeIds = new Set(filteredNodes.map(n => n.id));

    // If focal entity, do BFS to limit depth
    if (focalEntityId && depth < 100) {
      const visited = new Set<string>();
      const queue: { id: string; currentDepth: number }[] = [{ id: focalEntityId, currentDepth: 0 }];

      while (queue.length > 0) {
        const { id, currentDepth } = queue.shift()!;
        if (visited.has(id)) continue;
        visited.add(id);

        if (currentDepth < depth) {
          // Find connected entities
          for (const edge of fullData.edges) {
            if (edge.source === id && !visited.has(edge.target)) {
              queue.push({ id: edge.target, currentDepth: currentDepth + 1 });
            }
            if (edge.target === id && !visited.has(edge.source)) {
              queue.push({ id: edge.source, currentDepth: currentDepth + 1 });
            }
          }
        }
      }

      filteredNodes = filteredNodes.filter(n => visited.has(n.id));
      filteredNodeIds = new Set(filteredNodes.map(n => n.id));
    }

    const filteredEdges = fullData.edges.filter(
      e => filteredNodeIds.has(e.source) && filteredNodeIds.has(e.target)
    );

    return { nodes: filteredNodes, edges: filteredEdges };
  }
}
