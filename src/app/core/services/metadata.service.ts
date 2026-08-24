import { Injectable, inject } from '@angular/core';
import { ContentTag, MetadataOwnerType } from '../models';
import { DatabaseService } from './database.service';

@Injectable({ providedIn: 'root' })
export class MetadataService {
  private readonly db = inject(DatabaseService);

  listTags(universeId: string): Promise<ContentTag[]> {
    return this.db.select<ContentTag>(
      `SELECT t.*, COUNT(a.id) AS assigned FROM content_tags t
       LEFT JOIN content_tag_assignments a ON a.tag_id = t.id
       WHERE t.universe_id = $1 GROUP BY t.id ORDER BY t.name`, [universeId]);
  }

  listOwnerTags(ownerType: MetadataOwnerType, ownerId: string): Promise<ContentTag[]> {
    return this.db.select<ContentTag>(
      `SELECT t.* FROM content_tags t JOIN content_tag_assignments a ON a.tag_id = t.id
       WHERE a.owner_type = $1 AND a.owner_id = $2 ORDER BY t.name`, [ownerType, ownerId]);
  }

  async createTag(universeId: string, name: string, color: string): Promise<ContentTag> {
    const tag: ContentTag = { id: this.db.generateId(), universe_id: universeId, name: name.trim(), color, created_at: this.db.now() };
    await this.db.execute(
      `INSERT INTO content_tags (id, universe_id, name, color, created_at) VALUES ($1,$2,$3,$4,$5)`,
      [tag.id, tag.universe_id, tag.name, tag.color, tag.created_at]);
    return tag;
  }

  async setTag(ownerType: MetadataOwnerType, ownerId: string, tagId: string, assigned: boolean): Promise<void> {
    if (assigned) {
      await this.db.execute(
        `INSERT OR IGNORE INTO content_tag_assignments (id, tag_id, owner_type, owner_id, created_at) VALUES ($1,$2,$3,$4,$5)`,
        [this.db.generateId(), tagId, ownerType, ownerId, this.db.now()]);
    } else {
      await this.db.execute(`DELETE FROM content_tag_assignments WHERE tag_id = $1 AND owner_type = $2 AND owner_id = $3`, [tagId, ownerType, ownerId]);
    }
  }

  deleteTag(id: string): Promise<unknown> {
    return this.db.execute(`DELETE FROM content_tags WHERE id = $1`, [id]);
  }
}
