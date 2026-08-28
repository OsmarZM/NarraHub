import { Injectable, inject } from '@angular/core';
import { Universe, UniverseStats, UniverseWithStats } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { LegacyUniverseGateway } from './legacy-universe.gateway';
import { CreateUniverseInput, UniverseGateway, UpdateUniverseInput } from './universe.gateway';

/**
 * Adaptador do core Rust para Universo.
 *
 * A Fase 4 migra por ordem, não de uma vez: o que já é comando Rust chama o
 * comando; o que ainda não é delega para o adaptador legado. A lista de
 * delegações encolhe a cada fatia e chegar a zero é o sinal de que este
 * arquivo pode virar a implementação inteira e o legado pode sumir.
 *
 * Ordem 1 (leitura e estatísticas) — migrado: `list`, `get`, `getStats`.
 * Ordem 2 (universo) — migrado: `create`, `update`, `delete`.
 */
@Injectable({ providedIn: 'root' })
export class RustUniverseGateway implements UniverseGateway {
  private readonly core = inject(RustCoreService);
  private readonly legacy = inject(LegacyUniverseGateway);

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
    return this.legacy.create(input);
  }

  update(id: string, patch: UpdateUniverseInput): Promise<void> {
    return this.legacy.update(id, patch);
  }

  delete(id: string): Promise<void> {
    return this.legacy.delete(id);
  }
}
