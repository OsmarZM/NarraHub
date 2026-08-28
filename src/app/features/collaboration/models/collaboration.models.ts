/**
 * Tipos da colaboração.
 *
 * Moravam em `core/services/collaboration.service.ts`, junto do SQL que a Fase
 * 4 levou para o Rust. O serviço morreu; os tipos continuam sendo o contrato
 * do domínio, então vivem aqui.
 */

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

/** O que chega de um convidado. `camelCase` porque é payload de comando. */
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
