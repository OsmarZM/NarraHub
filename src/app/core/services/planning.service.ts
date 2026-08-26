import { Injectable, inject } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import {
  PlanningFieldDefinition,
  PlanningFieldType,
  PlanningFieldValues,
  PlanningItem,
  PlanningStatus,
} from '../models';
import { DatabaseService } from './database.service';

export interface PlanningCardUpdate {
  title: string;
  description: string;
  image: string;
  status: PlanningStatus;
  chapterId: string | null;
  fieldValues: PlanningFieldValues;
}

interface PlanningFieldLinkRow {
  field_definition_id: string;
  target_id: string;
}

@Injectable({ providedIn: 'root' })
export class PlanningService {
  private readonly db = inject(DatabaseService);

  list(universeId: string): Promise<PlanningItem[]> {
    return this.db.select<PlanningItem>(
      `SELECT p.*, c.title AS chapter_title, b.name AS book_name, s.name AS story_name
       FROM planning_items p
       LEFT JOIN chapters c ON c.id = p.chapter_id
       LEFT JOIN books b ON b.id = c.book_id
       LEFT JOIN stories s ON s.id = b.story_id
       WHERE p.universe_id = $1
       ORDER BY CASE p.status WHEN 'IDEIAS' THEN 0 WHEN 'PLANEJADO' THEN 1 WHEN 'ESCREVENDO' THEN 2 WHEN 'REVISAO' THEN 3 ELSE 4 END,
                p.sort_order, p.created_at`,
      [universeId],
    );
  }

  async create(
    universeId: string,
    title: string,
    description = '',
    chapterId: string | null = null,
    image = '',
  ): Promise<string> {
    const id = this.db.generateId();
    const now = this.db.now();
    await this.db.execute(
      `INSERT INTO planning_items
         (id, universe_id, chapter_id, title, description, image, custom_field_values, status, sort_order, created_at, updated_at)
       VALUES
         ($1, $2, $3, $4, $5, $6, '{}', 'IDEIAS',
          (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM planning_items WHERE universe_id = $2 AND status = 'IDEIAS'),
          $7, $7)`,
      [id, universeId, chapterId, title.trim(), description.trim(), image, now],
    );
    return id;
  }

  async saveCard(id: string, universeId: string, update: PlanningCardUpdate): Promise<void> {
    await invoke('planning_save_card', {
      request: {
        id,
        universeId,
        title: update.title.trim(),
        description: update.description.trim(),
        image: update.image,
        status: update.status,
        chapterId: update.chapterId,
        fieldValues: update.fieldValues,
      },
    });
  }

  async listFieldLinks(cardId: string): Promise<PlanningFieldValues> {
    const rows = await this.db.select<PlanningFieldLinkRow>(
      `SELECT field_definition_id,
              COALESCE(story_id, entity_id, tag_id) AS target_id
       FROM planning_field_links
       WHERE planning_item_id = $1
       ORDER BY created_at, id`,
      [cardId],
    );
    return rows.reduce<PlanningFieldValues>((values, row) => {
      const current = values[row.field_definition_id];
      values[row.field_definition_id] = [
        ...(Array.isArray(current) ? current : []),
        row.target_id,
      ];
      return values;
    }, {});
  }

  async saveOrder(universeId: string, items: PlanningItem[]): Promise<void> {
    if (!items.length) return;
    const ids = items.map((item) => item.id);
    const idPlaceholders = ids.map((_, index) => `$${index + 1}`);
    const statusOffset = ids.length;
    const orderOffset = statusOffset + ids.length;
    const nowIndex = orderOffset + ids.length + 1;
    const universeIndex = nowIndex + 1;
    const statusCases = items.map((item, index) => `WHEN ${idPlaceholders[index]} THEN $${statusOffset + index + 1}`).join(' ');
    const orderCases = items.map((item, index) => `WHEN ${idPlaceholders[index]} THEN $${orderOffset + index + 1}`).join(' ');
    const params: unknown[] = [
      ...ids,
      ...items.map((item) => item.status),
      ...items.map((item) => item.sort_order),
      this.db.now(),
      universeId,
    ];
    const result = await this.db.execute(
      `UPDATE planning_items
       SET status = CASE id ${statusCases} ELSE status END,
           sort_order = CASE id ${orderCases} ELSE sort_order END,
           updated_at = $${nowIndex}
       WHERE universe_id = $${universeIndex} AND id IN (${idPlaceholders.join(',')})`,
      params,
    );
    if (result.rowsAffected !== items.length) throw new Error('O quadro mudou enquanto o card era movido. Atualize e tente novamente.');
  }

  async delete(id: string, universeId: string): Promise<void> {
    await this.db.execute('DELETE FROM planning_items WHERE id = $1 AND universe_id = $2', [id, universeId]);
  }

  listFieldDefinitions(universeId: string): Promise<PlanningFieldDefinition[]> {
    return this.db.select<PlanningFieldDefinition>(
      `SELECT * FROM planning_field_definitions WHERE universe_id = $1 ORDER BY sort_order, created_at`,
      [universeId],
    );
  }

  async createFieldDefinition(
    universeId: string,
    name: string,
    fieldType: PlanningFieldType,
    options: string[],
  ): Promise<PlanningFieldDefinition> {
    const definition: PlanningFieldDefinition = {
      id: this.db.generateId(),
      universe_id: universeId,
      name: name.trim(),
      field_type: fieldType,
      options_json: JSON.stringify(options),
      sort_order: 0,
      created_at: this.db.now(),
      updated_at: this.db.now(),
    };
    const order = await this.db.selectOne<{ next_order: number }>(
      'SELECT COALESCE(MAX(sort_order), -1) + 1 AS next_order FROM planning_field_definitions WHERE universe_id = $1',
      [universeId],
    );
    definition.sort_order = Number(order?.next_order ?? 0);
    await this.db.execute(
      `INSERT INTO planning_field_definitions
         (id, universe_id, name, field_type, options_json, sort_order, created_at, updated_at)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$7)`,
      [definition.id, universeId, definition.name, fieldType, definition.options_json, definition.sort_order, definition.created_at],
    );
    return definition;
  }

  async renameFieldDefinition(id: string, universeId: string, name: string): Promise<void> {
    const result = await this.db.execute(
      'UPDATE planning_field_definitions SET name = $1, updated_at = $2 WHERE id = $3 AND universe_id = $4',
      [name.trim(), this.db.now(), id, universeId],
    );
    if (result.rowsAffected !== 1) throw new Error('O campo não existe mais neste universo.');
  }

  async deleteFieldDefinition(id: string, universeId: string): Promise<void> {
    await this.db.execute(
      'DELETE FROM planning_field_definitions WHERE id = $1 AND universe_id = $2',
      [id, universeId],
    );
  }
}
