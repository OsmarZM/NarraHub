import { Injectable, inject } from '@angular/core';
import { DatabaseService } from './database.service';

export type SharePermission = 'view' | 'comment' | 'edit';
export type ContributionStatus = 'pending' | 'approved' | 'rejected' | 'noted';

export interface CollaborationSession {
  id: string;
  title: string;
  permission: SharePermission;
  universe_ids: string;
  encryption_key: string;
  revoke_token: string;
  status: 'active' | 'ended' | 'revoked';
  created_at: string;
  expires_at: string;
  ended_at: string | null;
  pending_count?: number;
  note_count?: number;
}

export interface CollaborationContribution {
  id: string;
  session_id: string;
  sequence: number;
  contributor: string;
  kind: 'edit' | 'note';
  universe_id: string;
  target_type: 'universe' | 'chapter' | 'entity';
  target_id: string;
  target_label: string;
  field: string;
  original_value: string;
  proposed_value: string;
  message: string;
  status: ContributionStatus;
  created_at: string;
  reviewed_at: string | null;
}

export interface IncomingContribution {
  id: string;
  contributor: string;
  kind: 'edit' | 'note';
  universeId: string;
  targetType: 'universe' | 'chapter' | 'entity';
  targetId: string;
  targetLabel: string;
  field?: string;
  originalValue?: string;
  proposedValue?: string;
  message?: string;
  createdAt: string;
}

@Injectable({ providedIn: 'root' })
export class CollaborationService {
  private readonly db = inject(DatabaseService);

  async saveSession(session: {
    id: string; title: string; permission: SharePermission; universeIds: string[];
    encryptionKey: string; revokeToken: string; expiresAt: string;
  }): Promise<void> {
    await this.db.execute(
      `INSERT INTO collaboration_sessions (id, title, permission, universe_ids, encryption_key, revoke_token, status, created_at, expires_at)
       VALUES ($1, $2, $3, $4, $5, $6, 'active', $7, $8)
       ON CONFLICT(id) DO UPDATE SET title = excluded.title, permission = excluded.permission,
         universe_ids = excluded.universe_ids, encryption_key = excluded.encryption_key, revoke_token = excluded.revoke_token,
         status = 'active', expires_at = excluded.expires_at, ended_at = NULL`,
      [session.id, session.title, session.permission, JSON.stringify(session.universeIds), session.encryptionKey, session.revokeToken, this.db.now(), session.expiresAt],
    );
  }

  listSessions(): Promise<CollaborationSession[]> {
    return this.db.select<CollaborationSession>(
      `SELECT s.*,
        SUM(CASE WHEN c.kind = 'edit' AND c.status = 'pending' THEN 1 ELSE 0 END) AS pending_count,
        SUM(CASE WHEN c.kind = 'note' THEN 1 ELSE 0 END) AS note_count
       FROM collaboration_sessions s
       LEFT JOIN collaboration_contributions c ON c.session_id = s.id
       GROUP BY s.id ORDER BY s.created_at DESC`,
    );
  }

  listContributions(sessionId?: string): Promise<CollaborationContribution[]> {
    const where = sessionId ? 'WHERE session_id = $1' : '';
    return this.db.select<CollaborationContribution>(
      `SELECT * FROM collaboration_contributions ${where}
       ORDER BY CASE status WHEN 'pending' THEN 0 WHEN 'noted' THEN 1 ELSE 2 END, sequence DESC`,
      sessionId ? [sessionId] : [],
    );
  }

  async storeContribution(sessionId: string, sequence: number, contribution: IncomingContribution): Promise<boolean> {
    const status: ContributionStatus = contribution.kind === 'note' ? 'noted' : 'pending';
    const result = await this.db.execute(
      `INSERT OR IGNORE INTO collaboration_contributions
       (id, session_id, sequence, contributor, kind, universe_id, target_type, target_id, target_label,
        field, original_value, proposed_value, message, status, created_at)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)`,
      [contribution.id, sessionId, sequence, contribution.contributor || 'Convidado', contribution.kind,
        contribution.universeId, contribution.targetType, contribution.targetId, contribution.targetLabel,
        contribution.field || '', contribution.originalValue || '', contribution.proposedValue || '',
        contribution.message || '', status, contribution.createdAt || this.db.now()],
    );
    return result.rowsAffected > 0;
  }

  async endAllActive(status: 'ended' | 'revoked' = 'ended'): Promise<void> {
    await this.db.execute(
      `UPDATE collaboration_sessions SET status = $1, ended_at = $2 WHERE status = 'active'`,
      [status, this.db.now()],
    );
  }


  async endSession(id: string, status: 'ended' | 'revoked'): Promise<void> {
    await this.db.execute(
      `UPDATE collaboration_sessions SET status = $1, ended_at = $2 WHERE id = $3`,
      [status, this.db.now(), id],
    );
  }

  async review(id: string, decision: 'approved' | 'rejected'): Promise<void> {
    const contribution = await this.db.selectOne<CollaborationContribution>(
      `SELECT * FROM collaboration_contributions WHERE id = $1 AND kind = 'edit' AND status = 'pending'`,
      [id],
    );
    if (!contribution) return;
    if (decision === 'approved') await this.applyContribution(contribution);
    await this.db.execute(
      `UPDATE collaboration_contributions SET status = $1, reviewed_at = $2 WHERE id = $3`,
      [decision, this.db.now(), id],
    );
  }

  async approveAll(sessionId: string): Promise<number> {
    const pending = await this.db.select<CollaborationContribution>(
      `SELECT * FROM collaboration_contributions WHERE session_id = $1 AND kind = 'edit' AND status = 'pending' ORDER BY sequence`,
      [sessionId],
    );
    for (const contribution of pending) await this.review(contribution.id, 'approved');
    return pending.length;
  }

  private async applyContribution(item: CollaborationContribution): Promise<void> {
    const now = this.db.now();
    const allowed: Record<string, Set<string>> = {
      universe: new Set(['name', 'description']),
      chapter: new Set(['title', 'content', 'summary']),
      entity: new Set(['name', 'description', 'summary', 'canon_status']),
    };
    if (item.target_type === 'entity' && item.field.startsWith('attribute:')) {
      const key = item.field.slice('attribute:'.length).trim();
      if (!key || key.length > 120) throw new Error('Campo de ficha inválido.');
      const existing = await this.db.selectOne<{ id: string }>(
        'SELECT id FROM entity_attributes WHERE entity_id = $1 AND key = $2 COLLATE NOCASE',
        [item.target_id, key],
      );
      if (existing) await this.db.execute('UPDATE entity_attributes SET value = $1 WHERE id = $2', [item.proposed_value, existing.id]);
      else await this.db.execute(
        `INSERT INTO entity_attributes (id, entity_id, key, value, sort_order)
         VALUES ($1, $2, $3, $4, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM entity_attributes WHERE entity_id = $2))`,
        [this.db.generateId(), item.target_id, key, item.proposed_value],
      );
      await this.db.execute('UPDATE entities SET updated_at = $1 WHERE id = $2', [now, item.target_id]);
    } else {
      if (!allowed[item.target_type]?.has(item.field)) throw new Error('Alteração colaborativa fora do escopo permitido.');
      const table = item.target_type === 'universe' ? 'universes' : item.target_type === 'chapter' ? 'chapters' : 'entities';
      await this.db.execute(`UPDATE ${table} SET ${item.field} = $1, updated_at = $2 WHERE id = $3`, [item.proposed_value, now, item.target_id]);
    }
    await this.db.execute(
      `INSERT INTO change_log (id, universe_id, entity_type, entity_id, action, field, old_value, new_value, created_at)
       VALUES ($1, $2, $3, $4, 'update', $5, $6, $7, $8)`,
      [this.db.generateId(), item.universe_id, item.target_type, item.target_id, item.field, item.original_value, item.proposed_value, now],
    );
  }
}
