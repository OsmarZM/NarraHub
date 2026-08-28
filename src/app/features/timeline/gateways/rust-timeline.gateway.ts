import { Injectable, inject } from '@angular/core';
import { TimelineEvent } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { CreateTimelineEventInput } from '../models/timeline.models';
import { LegacyTimelineGateway } from './legacy-timeline.gateway';
import { TimelineGateway } from './timeline.gateway';

/**
 * Ordem 1 (leitura) — migrado: `list`.
 * Ordem 3 (timeline e planejamento) — migrado: `create`, `rename`, `delete`.
 */
@Injectable({ providedIn: 'root' })
export class RustTimelineGateway implements TimelineGateway {
  private readonly core = inject(RustCoreService);
  private readonly legacy = inject(LegacyTimelineGateway);

  list(universeId: string): Promise<TimelineEvent[]> {
    return this.core.call<TimelineEvent[]>('timeline_list', { universeId });
  }

  create(universeId: string, input: CreateTimelineEventInput): Promise<void> {
    return this.legacy.create(universeId, input);
  }

  rename(eventId: string, title: string): Promise<void> {
    return this.legacy.rename(eventId, title);
  }

  delete(eventId: string): Promise<void> {
    return this.legacy.delete(eventId);
  }
}
