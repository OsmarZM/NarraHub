// ============================================
// NarraHub — Entity Service
// ============================================

import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';
import {
  Entity, EntityAttribute, EntityTemplate, EntityWithDetails,
  EntityType, CanonStatus, DEFAULT_ATTRIBUTES,
  RelationWithEntity, MentionWithChapter, Relation,
} from '../models';

@Injectable({ providedIn: 'root' })
export class EntityService {
  private db = inject(DatabaseService);

  async create(
    universeId: string,
    type: EntityType,
    name: string,
    description: string = ''
  ): Promise<Entity> {
    const id = this.db.generateId();
    const now = this.db.now();

    await this.db.execute(
      `INSERT INTO entities (id, universe_id, type, name, description, summary, image, canon_status, created_at, updated_at)
       VALUES ($1, $2, $3, $4, $5, '', '', 'CANON', $6, $7)`,
      [id, universeId, type, name, description, now, now]
    );

    // Create default attributes based on type
    const defaultAttrs = DEFAULT_ATTRIBUTES[type] || [];
    for (let i = 0; i < defaultAttrs.length; i++) {
      await this.db.execute(
        `INSERT INTO entity_attributes (id, entity_id, key, value, sort_order)
         VALUES ($1, $2, $3, '', $4)`,
        [this.db.generateId(), id, defaultAttrs[i], i]
      );
    }

    // Also check for universe-specific templates
    const templates = await this.db.select<EntityTemplate>(
      `SELECT * FROM entity_templates WHERE universe_id = $1 AND entity_type = $2 ORDER BY sort_order`,
      [universeId, type]
    );
    for (const tmpl of templates) {
      // Only add if not already in defaults
      if (!defaultAttrs.includes(tmpl.attribute_key)) {
        await this.db.execute(
          `INSERT INTO entity_attributes (id, entity_id, key, value, sort_order)
           VALUES ($1, $2, $3, $4, $5)`,
          [this.db.generateId(), id, tmpl.attribute_key, tmpl.default_value, defaultAttrs.length + tmpl.sort_order]
        );
      }
    }

    return {
      id, universe_id: universeId, type, name, description, summary: '',
      image: '', canon_status: 'CANON', created_at: now, updated_at: now,
    };
  }

  async listByUniverse(universeId: string, type?: string): Promise<Entity[]> {
    if (type) {
      return this.db.select<Entity>(
        'SELECT * FROM entities WHERE universe_id = $1 AND type = $2 ORDER BY name',
        [universeId, type]
      );
    }
    return this.db.select<Entity>(
      'SELECT * FROM entities WHERE universe_id = $1 ORDER BY type, name',
      [universeId]
    );
  }

  async get(id: string): Promise<Entity | null> {
    return this.db.selectOne<Entity>('SELECT * FROM entities WHERE id = $1', [id]);
  }

  async getWithDetails(id: string): Promise<EntityWithDetails | null> {
    const entity = await this.get(id);
    if (!entity) return null;

    const attributes = await this.db.select<EntityAttribute>(
      'SELECT * FROM entity_attributes WHERE entity_id = $1 ORDER BY sort_order',
      [id]
    );

    // Get relations where this entity is source or target
    const relationsAsSource = await this.db.select<Relation & { target_name: string; target_type: string; target_image: string }>(
      `SELECT r.*, e.name as target_name, e.type as target_type, e.image as target_image
       FROM relations r
       JOIN entities e ON r.target_id = e.id
       WHERE r.source_id = $1`,
      [id]
    );

    const relationsAsTarget = await this.db.select<Relation & { source_name: string; source_type: string; source_image: string }>(
      `SELECT r.*, e.name as source_name, e.type as source_type, e.image as source_image
       FROM relations r
       JOIN entities e ON r.source_id = e.id
       WHERE r.target_id = $1`,
      [id]
    );

    const relations: RelationWithEntity[] = [
      ...relationsAsSource.map(r => ({
        ...r,
        source: entity,
        target: { id: r.target_id, name: (r as any).target_name, type: (r as any).target_type, image: (r as any).target_image } as Entity,
        bidirectional: !!r.bidirectional,
      })),
      ...relationsAsTarget.map(r => ({
        ...r,
        source: { id: r.source_id, name: (r as any).source_name, type: (r as any).source_type, image: (r as any).source_image } as Entity,
        target: entity,
        bidirectional: !!r.bidirectional,
      })),
    ];

    // Get mentions
    const mentions = await this.db.select<MentionWithChapter>(
      `SELECT m.*, c.title as chapter_title, b.name as book_name
       FROM mentions m
       JOIN chapters c ON m.chapter_id = c.id
       JOIN books b ON c.book_id = b.id
       WHERE m.entity_id = $1
       ORDER BY c.sort_order`,
      [id]
    );

    return { ...entity, attributes, relations, mentions };
  }

  async update(id: string, data: Partial<Pick<Entity, 'name' | 'description' | 'summary' | 'image' | 'canon_status'>>): Promise<void> {
    const now = this.db.now();
    const sets: string[] = ['updated_at = $1'];
    const params: unknown[] = [now];
    let idx = 2;

    for (const [key, value] of Object.entries(data)) {
      if (value !== undefined) {
        sets.push(`${key} = $${idx}`);
        params.push(value);
        idx++;
      }
    }

    params.push(id);
    await this.db.execute(
      `UPDATE entities SET ${sets.join(', ')} WHERE id = $${idx}`,
      params
    );
  }

  async delete(id: string): Promise<void> {
    await this.db.execute('DELETE FROM entities WHERE id = $1', [id]);
  }

  // ── Attributes ──────────────────────────────

  async setAttribute(entityId: string, key: string, value: string): Promise<void> {
    const existing = await this.db.selectOne<EntityAttribute>(
      'SELECT * FROM entity_attributes WHERE entity_id = $1 AND key = $2',
      [entityId, key]
    );

    if (existing) {
      await this.db.execute(
        'UPDATE entity_attributes SET value = $1 WHERE id = $2',
        [value, existing.id]
      );
    } else {
      const last = await this.db.selectOne<{ max_order: number }>(
        'SELECT COALESCE(MAX(sort_order), -1) as max_order FROM entity_attributes WHERE entity_id = $1',
        [entityId]
      );
      await this.db.execute(
        'INSERT INTO entity_attributes (id, entity_id, key, value, sort_order) VALUES ($1, $2, $3, $4, $5)',
        [this.db.generateId(), entityId, key, value, (last?.max_order ?? -1) + 1]
      );
    }

    // Update entity's updated_at
    await this.db.execute(
      'UPDATE entities SET updated_at = $1 WHERE id = $2',
      [this.db.now(), entityId]
    );
  }

  async removeAttribute(attributeId: string): Promise<void> {
    await this.db.execute('DELETE FROM entity_attributes WHERE id = $1', [attributeId]);
  }

  async saveAttribute(attribute: EntityAttribute): Promise<void> {
    if (attribute.id.startsWith('temp_')) {
      await this.setAttribute(attribute.entity_id, attribute.key, attribute.value);
      return;
    }
    await this.db.execute(
      `UPDATE entity_attributes SET key = $1, value = $2, sort_order = $3 WHERE id = $4 AND entity_id = $5`,
      [attribute.key.trim(), attribute.value, attribute.sort_order, attribute.id, attribute.entity_id]
    );
    await this.db.execute('UPDATE entities SET updated_at = $1 WHERE id = $2', [this.db.now(), attribute.entity_id]);
  }

  // ── Search (for mentions autocomplete) ──────

  async search(universeId: string, query: string, limit: number = 10): Promise<Entity[]> {
    return this.db.select<Entity>(
      `SELECT * FROM entities WHERE universe_id = $1 AND name LIKE $2 ORDER BY name LIMIT $3`,
      [universeId, `%${query}%`, limit]
    );
  }
}
