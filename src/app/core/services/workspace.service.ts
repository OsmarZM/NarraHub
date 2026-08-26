import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { HistoryEntry, RelationCard, TimelineEvent } from '../models';

@Injectable({ providedIn: 'root' })
export class WorkspaceService {
  private readonly db = inject(DatabaseService);

  listTimeline(universeId: string): Promise<TimelineEvent[]> {
    return this.db.select<TimelineEvent>(
      'SELECT * FROM timeline_events WHERE universe_id = $1 ORDER BY COALESCE(sort_key, 0), start_date, created_at',
      [universeId],
    );
  }

  async createTimeline(universeId: string, title: string, date: string, description = '', entityId: string | null = null, displayDate = '', sortKey = 0): Promise<void> {
    const id = this.db.generateId();
    const now = this.db.now();
    await this.db.execute(
      `INSERT INTO timeline_events (id, universe_id, title, description, event_type, start_date, end_date, entity_id, display_date, sort_key, created_at, updated_at)
       VALUES ($1, $2, $3, $4, 'MARCO', $5, '', $6, $7, $8, $9, $9)`,
      [id, universeId, title, description, date, entityId, displayDate, sortKey, now],
    );
  }

  async deleteTimeline(id: string): Promise<void> {
    await this.db.execute('DELETE FROM timeline_events WHERE id = $1', [id]);
  }

  async updateTimelineTitle(id: string, title: string): Promise<void> {
    await this.db.execute(
      'UPDATE timeline_events SET title = $1, updated_at = $2 WHERE id = $3',
      [title, this.db.now(), id],
    );
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
