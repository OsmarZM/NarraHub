import { Injectable, inject } from '@angular/core';
import {
  PlanningFieldDefinition, PlanningFieldType, PlanningFieldValues, PlanningItem,
} from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { LegacyPlanningGateway } from './legacy-planning.gateway';
import { PlanningCardUpdate, PlanningGateway } from './planning.gateway';

/**
 * Ordem 3 (timeline e planejamento) — migrado tudo, menos `saveCard`.
 *
 * `saveCard` já era comando Rust antes da Fase 4 (`planning_save_card`, em
 * `database/planning.rs`) e continua sendo chamado pelo adaptador legado. Ele
 * entra no core organizado junto da Ordem 4, quando os campos de relação
 * (`story`, `character`, `tags`) forem migrados — é lá que mora a validação
 * cruzada que ele faz, e mover as duas coisas separadas duplicaria a regra.
 */
@Injectable({ providedIn: 'root' })
export class RustPlanningGateway implements PlanningGateway {
  private readonly core = inject(RustCoreService);
  private readonly legacy = inject(LegacyPlanningGateway);

  list(universeId: string): Promise<PlanningItem[]> {
    return this.core.call<PlanningItem[]>('planning_list', { universeId });
  }

  create(universeId: string, title: string, description: string, chapterId: string | null, image: string): Promise<string> {
    return this.core.call<string>('planning_create', {
      universeId, title, description, chapterId, image,
    });
  }

  saveCard(id: string, universeId: string, update: PlanningCardUpdate): Promise<void> {
    return this.legacy.saveCard(id, universeId, update);
  }

  saveOrder(universeId: string, items: PlanningItem[]): Promise<void> {
    // O core só precisa de onde cada card ficou, não do card inteiro: mandar
    // o objeto completo faria o Rust receber campos que ele ignora e abriria
    // espaço para gravar por engano o que a tela não editou.
    return this.core.call<void>('planning_save_order', {
      universeId,
      placements: items.map((item) => ({
        id: item.id,
        status: item.status,
        sortOrder: item.sort_order,
      })),
    });
  }

  delete(id: string, universeId: string): Promise<void> {
    return this.core.call<void>('planning_delete', { id, universeId });
  }

  listFieldLinks(cardId: string): Promise<PlanningFieldValues> {
    return this.core.call<PlanningFieldValues>('planning_field_links', { cardId });
  }

  listFieldDefinitions(universeId: string): Promise<PlanningFieldDefinition[]> {
    return this.core.call<PlanningFieldDefinition[]>('planning_field_definitions', { universeId });
  }

  createFieldDefinition(universeId: string, name: string, fieldType: PlanningFieldType, options: string[]): Promise<PlanningFieldDefinition> {
    return this.core.call<PlanningFieldDefinition>('planning_field_definition_create', {
      universeId, name, fieldType, options,
    });
  }

  renameFieldDefinition(id: string, universeId: string, name: string): Promise<void> {
    return this.core.call<void>('planning_field_definition_rename', { id, universeId, name });
  }

  deleteFieldDefinition(id: string, universeId: string): Promise<void> {
    return this.core.call<void>('planning_field_definition_delete', { id, universeId });
  }
}
