import { TimelineEvent } from '../../../core/models';
import { CreateTimelineEventInput } from '../models/timeline.models';

export abstract class TimelineGateway {
  abstract list(universeId: string): Promise<TimelineEvent[]>;
  abstract create(universeId: string, input: CreateTimelineEventInput): Promise<void>;
  abstract rename(eventId: string, title: string): Promise<void>;
  abstract delete(eventId: string): Promise<void>;
}

