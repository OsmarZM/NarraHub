import { Injectable, inject } from '@angular/core';
import { TimelineEvent } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { CreateTimelineEventInput } from '../models/timeline.models';
import { TimelineGateway } from './timeline.gateway';

/**
 * Domínio migrado por inteiro — nada aqui delega mais para o legado.
 * Ordem 1 — `list`.  Ordem 3 — `create`, `rename`, `delete`.
 */
@Injectable({ providedIn: 'root' })
export class RustTimelineGateway implements TimelineGateway {
  private readonly core = inject(RustCoreService);

  list(universeId: string): Promise<TimelineEvent[]> {
    return this.core.call<TimelineEvent[]>('timeline_list', { universeId });
  }

  async create(universeId: string, input: CreateTimelineEventInput): Promise<void> {
    // O comando devolve o id do evento criado; o contrato do gateway não o
    // expõe porque a página recarrega a lista em seguida. Quando alguém
    // precisar dele, é só alargar o contrato — o dado já vem do Rust.
    await this.core.call<string>('timeline_create', { universeId, event: input });
  }

  rename(eventId: string, title: string): Promise<void> {
    return this.core.call<void>('timeline_rename', { id: eventId, title });
  }

  delete(eventId: string): Promise<void> {
    return this.core.call<void>('timeline_delete', { id: eventId });
  }
}
