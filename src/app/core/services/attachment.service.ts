import { Injectable, inject } from '@angular/core';
import { Attachment } from '../models';
import { DatabaseService } from './database.service';

@Injectable({ providedIn: 'root' })
export class AttachmentService {
  private readonly db = inject(DatabaseService);

  list(universeId: string, ownerType: Attachment['owner_type'], ownerId: string): Promise<Attachment[]> {
    return this.db.select<Attachment>(
      'SELECT * FROM attachments WHERE universe_id = $1 AND owner_type = $2 AND owner_id = $3 ORDER BY sort_order, created_at',
      [universeId, ownerType, ownerId],
    );
  }

  async create(universeId: string, ownerType: Attachment['owner_type'], ownerId: string, dataUrl: string, caption = ''): Promise<Attachment> {
    const id = this.db.generateId();
    const createdAt = this.db.now();
    const last = await this.db.selectOne<{ max_order: number }>(
      'SELECT COALESCE(MAX(sort_order), -1) AS max_order FROM attachments WHERE universe_id = $1 AND owner_type = $2 AND owner_id = $3',
      [universeId, ownerType, ownerId],
    );
    const sortOrder = (last?.max_order ?? -1) + 1;
    await this.db.execute(
      'INSERT INTO attachments (id, universe_id, owner_type, owner_id, data_url, caption, sort_order, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)',
      [id, universeId, ownerType, ownerId, dataUrl, caption, sortOrder, createdAt],
    );
    return { id, universe_id: universeId, owner_type: ownerType, owner_id: ownerId, data_url: dataUrl, caption, sort_order: sortOrder, created_at: createdAt };
  }

  delete(id: string): Promise<unknown> {
    return this.db.execute('DELETE FROM attachments WHERE id = $1', [id]);
  }
}
