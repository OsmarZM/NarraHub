// ============================================
// NarraHub — Story Service
// ============================================

import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { Story } from '../models';

@Injectable({ providedIn: 'root' })
export class StoryService {
  private db = inject(DatabaseService);

  async create(universeId: string, name: string, description: string = ''): Promise<Story> {
    const id = this.db.generateId();
    const now = this.db.now();
    const last = await this.db.selectOne<{ max_order: number }>(
      'SELECT COALESCE(MAX(sort_order), -1) as max_order FROM stories WHERE universe_id = $1',
      [universeId]
    );
    const sortOrder = (last?.max_order ?? -1) + 1;

    await this.db.execute(
      `INSERT INTO stories (id, universe_id, name, description, sort_order, created_at, updated_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7)`,
      [id, universeId, name, description, sortOrder, now, now]
    );

    return { id, universe_id: universeId, name, description, sort_order: sortOrder, created_at: now, updated_at: now };
  }

  async listByUniverse(universeId: string): Promise<Story[]> {
    return this.db.select<Story>('SELECT * FROM stories WHERE universe_id = $1 ORDER BY sort_order', [universeId]);
  }

  async get(id: string): Promise<Story | null> {
    return this.db.selectOne<Story>('SELECT * FROM stories WHERE id = $1', [id]);
  }

  async update(id: string, data: Partial<Pick<Story, 'name' | 'description'>>): Promise<void> {
    const now = this.db.now();
    const sets: string[] = ['updated_at = $1'];
    const params: unknown[] = [now];
    let idx = 2;
    for (const [key, value] of Object.entries(data)) {
      if (value !== undefined) { sets.push(`${key} = $${idx}`); params.push(value); idx++; }
    }
    params.push(id);
    await this.db.execute(`UPDATE stories SET ${sets.join(', ')} WHERE id = $${idx}`, params);
  }

  async delete(id: string): Promise<void> {
    await this.db.execute('DELETE FROM stories WHERE id = $1', [id]);
  }
}
