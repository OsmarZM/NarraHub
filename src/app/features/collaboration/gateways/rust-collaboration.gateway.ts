import { Injectable, inject } from '@angular/core';
import {
  CollaborationContribution, CollaborationSession, IncomingContribution,
} from '../models/collaboration.models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { CollaborationGateway, SaveCollaborationSessionInput } from './collaboration.gateway';

/**
 * Ordem 8 — colaboração e aplicação de propostas. Domínio migrado por inteiro.
 *
 * É o domínio onde o valor vem de fora, de um convidado com link. A lista de
 * campos que uma proposta aprovada pode escrever agora vive no core, e a
 * tabela e a coluna saem dessa lista — nunca do texto recebido.
 */
@Injectable({ providedIn: 'root' })
export class RustCollaborationGateway implements CollaborationGateway {
  private readonly core = inject(RustCoreService);

  listSessions(): Promise<CollaborationSession[]> {
    return this.core.call<CollaborationSession[]>('collaboration_sessions');
  }

  listContributions(sessionId?: string): Promise<CollaborationContribution[]> {
    return this.core.call<CollaborationContribution[]>('collaboration_contributions', {
      sessionId: sessionId ?? null,
    });
  }

  saveSession(session: SaveCollaborationSessionInput): Promise<void> {
    return this.core.call<void>('collaboration_save_session', { session });
  }

  storeContribution(sessionId: string, sequence: number, contribution: IncomingContribution): Promise<boolean> {
    return this.core.call<boolean>('collaboration_store_contribution', {
      sessionId, sequence, contribution,
    });
  }

  endAllActive(status: 'ended' | 'revoked'): Promise<void> {
    return this.core.call<void>('collaboration_end_all', { status });
  }

  endSession(id: string, status: 'ended' | 'revoked'): Promise<void> {
    return this.core.call<void>('collaboration_end_session', { id, status });
  }

  review(id: string, decision: 'approved' | 'rejected'): Promise<void> {
    return this.core.call<void>('collaboration_review', { id, decision });
  }

  approveAll(sessionId: string): Promise<number> {
    return this.core.call<number>('collaboration_approve_all', { sessionId });
  }
}
