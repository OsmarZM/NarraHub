import { Injectable, inject } from '@angular/core';
import { CollaborationContribution, CollaborationService, CollaborationSession, IncomingContribution } from '../../../core/services/collaboration.service';
import { CollaborationGateway, SaveCollaborationSessionInput } from './collaboration.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyCollaborationGateway implements CollaborationGateway {
  private readonly collaborationService = inject(CollaborationService);

  listSessions(): Promise<CollaborationSession[]> {
    return this.collaborationService.listSessions();
  }

  listContributions(sessionId?: string): Promise<CollaborationContribution[]> {
    return this.collaborationService.listContributions(sessionId);
  }

  saveSession(session: SaveCollaborationSessionInput): Promise<void> {
    return this.collaborationService.saveSession(session);
  }

  storeContribution(sessionId: string, sequence: number, contribution: IncomingContribution): Promise<boolean> {
    return this.collaborationService.storeContribution(sessionId, sequence, contribution);
  }

  endAllActive(status: 'ended' | 'revoked'): Promise<void> {
    return this.collaborationService.endAllActive(status);
  }

  endSession(id: string, status: 'ended' | 'revoked'): Promise<void> {
    return this.collaborationService.endSession(id, status);
  }

  review(id: string, decision: 'approved' | 'rejected'): Promise<void> {
    return this.collaborationService.review(id, decision);
  }

  approveAll(sessionId: string): Promise<number> {
    return this.collaborationService.approveAll(sessionId);
  }
}
