import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { HistoryEntry, PlanningItem, PlanningStatus, RelationCard, TimelineEvent } from '../models';

@Injectable({ providedIn: 'root' })
export class WorkspaceService {
  private readonly db = inject(DatabaseService);

  listTimeline(universeId: string): Promise<TimelineEvent[]> {
    return this.db.select<TimelineEvent>(
      'SELECT * FROM timeline_events WHERE universe_id = $1 ORDER BY start_date, created_at',
      [universeId],
    );
  }

  async createTimeline(universeId: string, title: string, date: string, description = ''): Promise<void> {
    const id = this.db.generateId();
    const now = this.db.now();
    await this.db.execute(
      `INSERT INTO timeline_events (id, universe_id, title, description, event_type, start_date, end_date, created_at, updated_at)
       VALUES ($1, $2, $3, $4, 'MARCO', $5, '', $6, $6)`,
      [id, universeId, title, description, date, now],
    );
  }

  async deleteTimeline(id: string): Promise<void> {
    await this.db.execute('DELETE FROM timeline_events WHERE id = $1', [id]);
  }

  listPlanning(universeId: string): Promise<PlanningItem[]> {
    return this.db.select<PlanningItem>(
      `SELECT * FROM planning_items WHERE universe_id = $1
       ORDER BY CASE status WHEN 'IDEIAS' THEN 0 WHEN 'PLANEJADO' THEN 1 WHEN 'ESCREVENDO' THEN 2 WHEN 'REVISAO' THEN 3 ELSE 4 END, sort_order, created_at`,
      [universeId],
    );
  }

  async createPlanning(universeId: string, title: string, description = ''): Promise<void> {
    const id = this.db.generateId();
    const now = this.db.now();
    await this.db.execute(
      `INSERT INTO planning_items (id, universe_id, title, description, status, created_at, updated_at)
       VALUES ($1, $2, $3, $4, 'IDEIAS', $5, $5)`,
      [id, universeId, title, description, now],
    );
  }

  async movePlanning(id: string, status: PlanningStatus): Promise<void> {
    await this.db.execute('UPDATE planning_items SET status = $1, updated_at = $2 WHERE id = $3', [status, this.db.now(), id]);
  }

  async deletePlanning(id: string): Promise<void> {
    await this.db.execute('DELETE FROM planning_items WHERE id = $1', [id]);
  }

  listRelations(universeId: string): Promise<RelationCard[]> {
    return this.db.select<RelationCard>(
      `SELECT r.*, source.name AS source_name, source.type AS source_type,
              target.name AS target_name, target.type AS target_type
       FROM relations r
       JOIN entities source ON source.id = r.source_id
       JOIN entities target ON target.id = r.target_id
       WHERE r.universe_id = $1 ORDER BY r.created_at DESC`,
      [universeId],
    );
  }

  async createRelation(universeId: string, sourceId: string, targetId: string, label: string): Promise<void> {
    const id = this.db.generateId();
    await this.db.execute(
      `INSERT INTO relations (id, universe_id, source_id, target_id, type, label, bidirectional, importance, created_at)
       VALUES ($1, $2, $3, $4, 'custom', $5, 0, 'normal', $6)`,
      [id, universeId, sourceId, targetId, label, this.db.now()],
    );
  }

  async deleteRelation(id: string): Promise<void> {
    await this.db.execute('DELETE FROM relations WHERE id = $1', [id]);
  }

  listHistory(universeId: string): Promise<HistoryEntry[]> {
    return this.db.select<HistoryEntry>(
      `SELECT c.*, COALESCE(u.name, s.name, b.name, ch.title, e.name, c.entity_id) AS display_name
       FROM change_log c
       LEFT JOIN universes u ON c.entity_type = 'universe' AND u.id = c.entity_id
       LEFT JOIN stories s ON c.entity_type = 'story' AND s.id = c.entity_id
       LEFT JOIN books b ON c.entity_type = 'book' AND b.id = c.entity_id
       LEFT JOIN chapters ch ON c.entity_type = 'chapter' AND ch.id = c.entity_id
       LEFT JOIN entities e ON c.entity_type = 'entity' AND e.id = c.entity_id
       WHERE c.universe_id = $1 ORDER BY c.created_at DESC LIMIT 100`,
      [universeId],
    );
  }
}
