import { HistoryEntry } from '../../../core/models';

export abstract class HistoryGateway {
  abstract listRecent(universeId: string): Promise<HistoryEntry[]>;
}

