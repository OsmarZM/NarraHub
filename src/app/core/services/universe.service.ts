// ============================================
// NarraHub — Universe Service
// ============================================

import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { Universe, UniverseStats, UniverseWithStats } from '../models';

@Injectable({ providedIn: 'root' })
export class UniverseService {
  private db = inject(DatabaseService);

  async create(name: string, description: string = ''): Promise<Universe> {
    const id = this.db.generateId();
    const now = this.db.now();

    await this.db.execute(
      `INSERT INTO universes (id, name, description, created_at, updated_at)
       VALUES ($1, $2, $3, $4, $5)`,
      [id, name, description, now, now]
    );

    return { id, name, description, cover_image: '', created_at: now, updated_at: now };
  }

  async list(): Promise<UniverseWithStats[]> {
    const universes = await this.db.select<Universe>('SELECT * FROM universes ORDER BY updated_at DESC');
    const result: UniverseWithStats[] = [];

    for (const u of universes) {
      const stats = await this.getStats(u.id);
      result.push({ ...u, stats });
    }

    return result;
  }

  async get(id: string): Promise<Universe | null> {
    return this.db.selectOne<Universe>('SELECT * FROM universes WHERE id = $1', [id]);
  }

  async update(id: string, data: Partial<Pick<Universe, 'name' | 'description' | 'cover_image'>>): Promise<void> {
    const now = this.db.now();
    const sets: string[] = ['updated_at = $1'];
    const params: unknown[] = [now];
    let paramIdx = 2;

    if (data.name !== undefined) {
      sets.push(`name = $${paramIdx}`);
      params.push(data.name);
      paramIdx++;
    }
    if (data.description !== undefined) {
      sets.push(`description = $${paramIdx}`);
      params.push(data.description);
      paramIdx++;
    }
    if (data.cover_image !== undefined) {
      sets.push(`cover_image = $${paramIdx}`);
      params.push(data.cover_image);
      paramIdx++;
    }

    params.push(id);
    await this.db.execute(
      `UPDATE universes SET ${sets.join(', ')} WHERE id = $${paramIdx}`,
      params
    );
  }

  async delete(id: string): Promise<void> {
    await this.db.execute('DELETE FROM universes WHERE id = $1', [id]);
  }

  async getStats(universeId: string): Promise<UniverseStats> {
    const wordResult = await this.db.selectOne<{ total: number }>(
      `SELECT COALESCE(SUM(c.word_count), 0) as total
       FROM chapters c
       JOIN books b ON c.book_id = b.id
       JOIN stories s ON b.story_id = s.id
       WHERE s.universe_id = $1`,
      [universeId]
    );

    const chapterResult = await this.db.selectOne<{ total: number }>(
      `SELECT COUNT(*) as total
       FROM chapters c
       JOIN books b ON c.book_id = b.id
       JOIN stories s ON b.story_id = s.id
       WHERE s.universe_id = $1`,
      [universeId]
    );

    const storyResult = await this.db.selectOne<{ total: number }>(
      `SELECT COUNT(*) as total FROM stories WHERE universe_id = $1`,
      [universeId]
    );

    const bookResult = await this.db.selectOne<{ total: number }>(
      `SELECT COUNT(*) as total FROM books b JOIN stories s ON b.story_id = s.id WHERE s.universe_id = $1`,
      [universeId]
    );

    const entityResult = await this.db.selectOne<{ total: number }>(
      `SELECT COUNT(*) as total FROM entities WHERE universe_id = $1`,
      [universeId]
    );

    const entityCounts: Record<string, number> = {};
    const typeCounts = await this.db.select<{ type: string; count: number }>(
      `SELECT type, COUNT(*) as count FROM entities WHERE universe_id = $1 GROUP BY type`,
      [universeId]
    );
    for (const tc of typeCounts) {
      entityCounts[tc.type] = tc.count;
    }

    return {
      total_words: wordResult?.total ?? 0,
      total_chapters: chapterResult?.total ?? 0,
      total_stories: storyResult?.total ?? 0,
      total_books: bookResult?.total ?? 0,
      total_entities: entityResult?.total ?? 0,
      entity_counts: entityCounts,
    };
  }
}
