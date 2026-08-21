// ============================================
// NarraHub — Book Service
// ============================================

import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { Book, BookOption } from '../models';

@Injectable({ providedIn: 'root' })
export class BookService {
  private db = inject(DatabaseService);

  async create(storyId: string, name: string, description: string = ''): Promise<Book> {
    const id = this.db.generateId();
    const now = this.db.now();
    const last = await this.db.selectOne<{ max_order: number }>(
      'SELECT COALESCE(MAX(sort_order), -1) as max_order FROM books WHERE story_id = $1',
      [storyId]
    );
    const sortOrder = (last?.max_order ?? -1) + 1;

    await this.db.execute(
      `INSERT INTO books (id, story_id, name, description, cover_image, sort_order, created_at, updated_at)
       VALUES ($1, $2, $3, $4, '', $5, $6, $7)`,
      [id, storyId, name, description, sortOrder, now, now]
    );

    return { id, story_id: storyId, name, description, cover_image: '', sort_order: sortOrder, created_at: now, updated_at: now };
  }

  async listByStory(storyId: string): Promise<Book[]> {
    return this.db.select<Book>('SELECT * FROM books WHERE story_id = $1 ORDER BY sort_order', [storyId]);
  }

  async listByUniverse(universeId: string): Promise<BookOption[]> {
    return this.db.select<BookOption>(
      `SELECT b.*, s.name AS story_name
       FROM books b
       JOIN stories s ON s.id = b.story_id
       WHERE s.universe_id = $1
       ORDER BY s.sort_order, b.sort_order`,
      [universeId],
    );
  }

  async get(id: string): Promise<Book | null> {
    return this.db.selectOne<Book>('SELECT * FROM books WHERE id = $1', [id]);
  }

  async update(id: string, data: Partial<Pick<Book, 'name' | 'description' | 'cover_image'>>): Promise<void> {
    const now = this.db.now();
    const sets: string[] = ['updated_at = $1'];
    const params: unknown[] = [now];
    let idx = 2;
    for (const [key, value] of Object.entries(data)) {
      if (value !== undefined) { sets.push(`${key} = $${idx}`); params.push(value); idx++; }
    }
    params.push(id);
    await this.db.execute(`UPDATE books SET ${sets.join(', ')} WHERE id = $${idx}`, params);
  }

  async delete(id: string): Promise<void> {
    await this.db.execute('DELETE FROM books WHERE id = $1', [id]);
  }
}
