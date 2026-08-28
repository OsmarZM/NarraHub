import { Injectable, inject } from '@angular/core';
import { HistoryEntry } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { HistoryGateway } from './history.gateway';

/**
 * Histórico é só leitura, então a Ordem 1 já o migra inteiro — não sobra nada
 * para delegar ao legado.
 */
@Injectable({ providedIn: 'root' })
export class RustHistoryGateway implements HistoryGateway {
  private readonly core = inject(RustCoreService);

  listRecent(universeId: string): Promise<HistoryEntry[]> {
    return this.core.call<HistoryEntry[]>('history_list', { universeId });
  }
}
