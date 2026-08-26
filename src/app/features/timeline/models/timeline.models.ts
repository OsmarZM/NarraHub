export interface CreateTimelineEventInput {
  title: string;
  date: string;
  description: string;
  entityId: string | null;
  displayDate: string;
  sortKey: number;
}

