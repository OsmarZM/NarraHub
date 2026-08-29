import { Injectable, inject } from '@angular/core';
import {
  PlanningFieldDefinition, PlanningFieldScope, PlanningFieldType, PlanningFieldValues, PlanningItem,
} from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { PlanningCardUpdate, PlanningGateway } from './planning.gateway';

/**
 * Domínio migrado por inteiro (Ordem 3).
 *
 * `saveCard` chama `planning_save_card`, que já era comando Rust antes da Fase
 * 4 e mora em `database/planning.rs` em vez de `application/planning_service`.
 * Ele ficou onde está de propósito: é o único ponto do core que valida os
 * campos de relação do card contra as definições do universo, e mover essa
 * validação sem reescrevê-la seria trocar um lugar testado por outro igual.
 * A reorganização dele para a camada de aplicação está anotada no plano.
 */
@Injectable({ providedIn: 'root' })
export class RustPlanningGateway implements PlanningGateway {
  private readonly core = inject(RustCoreService);

  list(universeId: string): Promise<PlanningItem[]> {
    return this.core.call<PlanningItem[]>('planning_list', { universeId });
  }

  create(universeId: string, title: string, description: string, chapterId: string | null, image: string): Promise<string> {
    return this.core.call<string>('planning_create', {
      universeId, title, description, chapterId, image,
    });
  }

  saveCard(id: string, universeId: string, update: PlanningCardUpdate): Promise<void> {
    return this.core.call<void>('planning_save_card', {
      request: {
        id,
        universeId,
        title: update.title.trim(),
        description: update.description.trim(),
        image: update.image,
        status: update.status,
        chapterId: update.chapterId,
        fieldValues: update.fieldValues,
      },
    });
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

  /**
   * Sem `cardId` o core devolve o catálogo inteiro do universo, que é o que a
   * store guarda: a ficha aberta filtra o que mostra a partir dele.
   */
  listFieldDefinitions(universeId: string): Promise<PlanningFieldDefinition[]> {
    return this.core.call<PlanningFieldDefinition[]>('planning_field_definitions', { universeId, cardId: null });
  }

  createFieldDefinition(
    universeId: string,
    name: string,
    fieldType: PlanningFieldType,
    options: string[],
    scope: PlanningFieldScope,
    cardId: string | null,
  ): Promise<PlanningFieldDefinition> {
    return this.core.call<PlanningFieldDefinition>('planning_field_definition_create', {
      universeId, name, fieldType, options, scope, cardId,
    });
  }

  renameFieldDefinition(id: string, universeId: string, name: string): Promise<void> {
    return this.core.call<void>('planning_field_definition_rename', { id, universeId, name });
  }

  setFieldDefinitionScope(id: string, universeId: string, scope: PlanningFieldScope, cardId: string | null): Promise<void> {
    return this.core.call<void>('planning_field_definition_set_scope', { id, universeId, scope, cardId });
  }

  deleteFieldDefinition(id: string, universeId: string): Promise<void> {
    return this.core.call<void>('planning_field_definition_delete', { id, universeId });
  }
}
