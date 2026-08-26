import { Injectable, inject } from '@angular/core';
import { HistoryEntry } from '../../../core/models';
import { WorkspaceService } from '../../../core/services/workspace.service';
import { HistoryGateway } from './history.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyHistoryGateway implements HistoryGateway {
  private readonly workspace = inject(WorkspaceService);

  listRecent(universeId: string): Promise<HistoryEntry[]> {
    return this.workspace.listHistory(universeId);
  }
}

