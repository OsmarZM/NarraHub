import { Injectable, inject } from '@angular/core';
import { TimelineEvent } from '../../../core/models';
import { WorkspaceService } from '../../../core/services/workspace.service';
import { TimelineGateway } from './timeline.gateway';
import { CreateTimelineEventInput } from '../models/timeline.models';

@Injectable({ providedIn: 'root' })
export class LegacyTimelineGateway implements TimelineGateway {
  private readonly workspace = inject(WorkspaceService);

  list(universeId: string): Promise<TimelineEvent[]> {
    return this.workspace.listTimeline(universeId);
  }

  create(universeId: string, input: CreateTimelineEventInput): Promise<void> {
    return this.workspace.createTimeline(
      universeId,
      input.title,
      input.date,
      input.description,
      input.entityId,
      input.displayDate,
      input.sortKey,
    );
  }

  rename(eventId: string, title: string): Promise<void> {
    return this.workspace.updateTimelineTitle(eventId, title);
  }

  delete(eventId: string): Promise<void> {
    return this.workspace.deleteTimeline(eventId);
  }
}

