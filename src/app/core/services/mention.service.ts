// ============================================
// NarraHub — Mention Service
// ============================================

import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { Mention, MentionOccurrence } from '../models';

@Injectable({ providedIn: 'root' })
export class MentionService {
  private db = inject(DatabaseService);

  async create(chapterId: string, entityId: string): Promise<Mention> {
    // Check if already exists
    const existing = await this.db.selectOne<Mention>(
      'SELECT * FROM mentions WHERE chapter_id = $1 AND entity_id = $2',
      [chapterId, entityId]
    );
    if (existing) return existing;

    const id = this.db.generateId();
    const now = this.db.now();

    await this.db.execute(
      'INSERT INTO mentions (id, chapter_id, entity_id, created_at) VALUES ($1, $2, $3, $4)',
      [id, chapterId, entityId, now]
    );

    return { id, chapter_id: chapterId, entity_id: entityId, created_at: now };
  }

  async listByChapter(chapterId: string): Promise<Mention[]> {
    return this.db.select<Mention>(
      'SELECT * FROM mentions WHERE chapter_id = $1',
      [chapterId]
    );
  }

  async listByEntity(entityId: string): Promise<Mention[]> {
    return this.db.select<Mention>(
      'SELECT * FROM mentions WHERE entity_id = $1',
      [entityId]
    );
  }

  async listByUniverse(universeId: string): Promise<MentionOccurrence[]> {
    return this.db.select<MentionOccurrence>(
      `SELECT m.*, c.title AS chapter_title, b.name AS book_name, s.name AS story_name,
              s.id AS story_id, b.id AS book_id, c.sort_order AS chapter_sort_order,
              b.sort_order AS book_sort_order, s.sort_order AS story_sort_order
       FROM mentions m
       JOIN chapters c ON c.id = m.chapter_id
       JOIN books b ON b.id = c.book_id
       JOIN stories s ON s.id = b.story_id
       WHERE s.universe_id = $1
       ORDER BY s.sort_order, b.sort_order, c.sort_order, m.created_at`,
      [universeId],
    );
  }

  async syncChapterMentions(chapterId: string, entityIds: string[]): Promise<void> {
    // Remove mentions not in the new list
    if (entityIds.length > 0) {
      const placeholders = entityIds.map((_, i) => `$${i + 2}`).join(',');
      await this.db.execute(
        `DELETE FROM mentions WHERE chapter_id = $1 AND entity_id NOT IN (${placeholders})`,
        [chapterId, ...entityIds]
      );
    } else {
      await this.db.execute('DELETE FROM mentions WHERE chapter_id = $1', [chapterId]);
    }

    // Add new mentions
    for (const entityId of entityIds) {
      await this.create(chapterId, entityId);
    }
  }

  async delete(id: string): Promise<void> {
    await this.db.execute('DELETE FROM mentions WHERE id = $1', [id]);
  }
}
