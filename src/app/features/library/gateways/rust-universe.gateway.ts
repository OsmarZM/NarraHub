import { Injectable, inject } from '@angular/core';
import { Universe, UniverseStats, UniverseWithStats } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { CreateUniverseInput, UniverseGateway, UpdateUniverseInput } from './universe.gateway';

/**
 * Adaptador do core Rust para Universo.
 *
 * Domínio migrado por inteiro: nada aqui delega mais para o legado, então
 * `LegacyUniverseGateway` e `UniverseService` já podem sair quando o resto da
 * Fase 4 liberar o `DatabaseService`.
 *
 * Ordem 1 — `list`, `get`, `getStats`.  Ordem 2 — `create`, `update`, `delete`.
 */
@Injectable({ providedIn: 'root' })
export class RustUniverseGateway implements UniverseGateway {
  private readonly core = inject(RustCoreService);

  list(): Promise<UniverseWithStats[]> {
    return this.core.call<UniverseWithStats[]>('universe_list');
  }

  get(id: string): Promise<Universe | null> {
    return this.core.call<Universe | null>('universe_get', { id });
  }

  getStats(id: string): Promise<UniverseStats> {
    return this.core.call<UniverseStats>('universe_stats', { universeId: id });
  }

  create(input: CreateUniverseInput): Promise<Universe> {
    return this.core.call<Universe>('universe_create', {
      name: input.name,
      description: input.description,
      coverImage: input.coverImage ?? '',
    });
  }

  update(id: string, patch: UpdateUniverseInput): Promise<void> {
    return this.core.call<void>('universe_update', { id, patch });
  }

  delete(id: string): Promise<void> {
    return this.core.call<void>('universe_delete', { id });
  }
}
