import { CollaborationContribution, CollaborationSession, IncomingContribution, SharePermission } from '../../../core/services/collaboration.service';

export interface SaveCollaborationSessionInput {
  id: string;
  title: string;
  permission: SharePermission;
  universeIds: string[];
  encryptionKey: string;
  revokeToken: string;
  expiresAt: string;
}

export abstract class CollaborationGateway {
  abstract listSessions(): Promise<CollaborationSession[]>;
  abstract listContributions(sessionId?: string): Promise<CollaborationContribution[]>;
  abstract saveSession(session: SaveCollaborationSessionInput): Promise<void>;
  abstract storeContribution(sessionId: string, sequence: number, contribution: IncomingContribution): Promise<boolean>;
  abstract endAllActive(status: 'ended' | 'revoked'): Promise<void>;
  abstract endSession(id: string, status: 'ended' | 'revoked'): Promise<void>;
  abstract review(id: string, decision: 'approved' | 'rejected'): Promise<void>;
  abstract approveAll(sessionId: string): Promise<number>;
}
