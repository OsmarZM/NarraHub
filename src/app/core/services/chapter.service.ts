// ============================================
// NarraHub — Chapter Service
// ============================================

import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import { Chapter, ChapterOption, ChapterStatus, CanonStatus } from '../models';

@Injectable({ providedIn: 'root' })
export class ChapterService {
  private db = inject(DatabaseService);

  async create(bookId: string, title: string): Promise<Chapter> {
    const id = this.db.generateId();
    const now = this.db.now();

    // Get next sort order
    const last = await this.db.selectOne<{ max_order: number }>(
      `SELECT COALESCE(MAX(sort_order), -1) as max_order FROM chapters WHERE book_id = $1`,
      [bookId]
    );
    const sortOrder = (last?.max_order ?? -1) + 1;

    await this.db.execute(
      `INSERT INTO chapters (id, book_id, title, content, word_count, status, canon_status, sort_order, created_at, updated_at)
       VALUES ($1, $2, $3, '', 0, 'IDEIA', 'CANON', $4, $5, $6)`,
      [id, bookId, title, sortOrder, now, now]
    );

    return {
      id, book_id: bookId, title, content: '', word_count: 0,
      status: 'IDEIA', canon_status: 'CANON', sort_order: sortOrder,
      created_at: now, updated_at: now,
    };
  }

  async listByBook(bookId: string): Promise<Chapter[]> {
    return this.db.select<Chapter>(
      'SELECT * FROM chapters WHERE book_id = $1 ORDER BY sort_order ASC',
      [bookId]
    );
  }

  async listByUniverse(universeId: string): Promise<ChapterOption[]> {
    return this.db.select<ChapterOption>(
      `SELECT c.*, b.name AS book_name, b.story_id, s.name AS story_name
       FROM chapters c
       JOIN books b ON b.id = c.book_id
       JOIN stories s ON s.id = b.story_id
       WHERE s.universe_id = $1
       ORDER BY s.sort_order, b.sort_order, c.sort_order`,
      [universeId],
    );
  }

  async get(id: string): Promise<Chapter | null> {
    return this.db.selectOne<Chapter>('SELECT * FROM chapters WHERE id = $1', [id]);
  }

  async updateContent(id: string, content: string, wordCount: number): Promise<void> {
    const now = this.db.now();
    await this.db.execute(
      `UPDATE chapters SET content = $1, word_count = $2, updated_at = $3 WHERE id = $4`,
      [content, wordCount, now, id]
    );
  }

  async updateTitle(id: string, title: string): Promise<void> {
    const now = this.db.now();
    await this.db.execute(
      `UPDATE chapters SET title = $1, updated_at = $2 WHERE id = $3`,
      [title, now, id]
    );
  }

  async updateStatus(id: string, status: ChapterStatus): Promise<void> {
    const now = this.db.now();
    await this.db.execute(
      `UPDATE chapters SET status = $1, updated_at = $2 WHERE id = $3`,
      [status, now, id]
    );
  }

  async updateCanonStatus(id: string, canonStatus: CanonStatus): Promise<void> {
    const now = this.db.now();
    await this.db.execute(
      `UPDATE chapters SET canon_status = $1, updated_at = $2 WHERE id = $3`,
      [canonStatus, now, id]
    );
  }

  async reorder(bookId: string, chapterIds: string[]): Promise<void> {
    for (let i = 0; i < chapterIds.length; i++) {
      await this.db.execute(
        `UPDATE chapters SET sort_order = $1 WHERE id = $2 AND book_id = $3`,
        [i, chapterIds[i], bookId]
      );
    }
  }

  async delete(id: string): Promise<void> {
    await this.db.execute('DELETE FROM chapters WHERE id = $1', [id]);
  }
}
