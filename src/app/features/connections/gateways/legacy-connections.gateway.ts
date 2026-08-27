import { Injectable, inject } from '@angular/core';
import { RelationCard } from '../../../core/models';
import { WorkspaceService } from '../../../core/services/workspace.service';
import { ConnectionsGateway } from './connections.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyConnectionsGateway implements ConnectionsGateway {
  private readonly workspaceService = inject(WorkspaceService);

  listRelations(universeId: string): Promise<RelationCard[]> {
    return this.workspaceService.listRelations(universeId);
  }

  createRelation(universeId: string, sourceId: string, targetId: string, label: string): Promise<void> {
    return this.workspaceService.createRelation(universeId, sourceId, targetId, label);
  }

  deleteRelation(id: string): Promise<void> {
    return this.workspaceService.deleteRelation(id);
  }
}
