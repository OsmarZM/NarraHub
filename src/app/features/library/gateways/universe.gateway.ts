import { Universe, UniverseStats, UniverseWithStats } from '../../../core/models';

export interface CreateUniverseInput {
  name: string;
  description: string;
  coverImage?: string;
}

export interface UpdateUniverseInput {
  name?: string;
  description?: string;
  coverImage?: string;
}

export abstract class UniverseGateway {
  abstract list(): Promise<UniverseWithStats[]>;
  abstract get(id: string): Promise<Universe | null>;
  abstract create(input: CreateUniverseInput): Promise<Universe>;
  abstract update(id: string, patch: UpdateUniverseInput): Promise<void>;
  abstract delete(id: string): Promise<void>;
  abstract getStats(id: string): Promise<UniverseStats>;
}
