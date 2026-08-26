import { Injectable, inject } from '@angular/core';
import { Universe, UniverseStats, UniverseWithStats } from '../../../core/models';
import { UniverseService } from '../../../core/services/universe.service';
import { CreateUniverseInput, UniverseGateway, UpdateUniverseInput } from './universe.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyUniverseGateway implements UniverseGateway {
  private readonly universeService = inject(UniverseService);

  list(): Promise<UniverseWithStats[]> {
    return this.universeService.list();
  }

  get(id: string): Promise<Universe | null> {
    return this.universeService.get(id);
  }

  async create(input: CreateUniverseInput): Promise<Universe> {
    const universe = await this.universeService.create(input.name, input.description);
    if (!input.coverImage) return universe;
    await this.universeService.update(universe.id, { cover_image: input.coverImage });
    return { ...universe, cover_image: input.coverImage };
  }

  update(id: string, patch: UpdateUniverseInput): Promise<void> {
    return this.universeService.update(id, {
      name: patch.name,
      description: patch.description,
      cover_image: patch.coverImage,
    });
  }

  delete(id: string): Promise<void> {
    return this.universeService.delete(id);
  }

  getStats(id: string): Promise<UniverseStats> {
    return this.universeService.getStats(id);
  }
}
