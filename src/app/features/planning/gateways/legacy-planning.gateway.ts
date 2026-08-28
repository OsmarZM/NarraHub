import { Injectable, inject } from '@angular/core';
import {
  PlanningFieldDefinition, PlanningFieldType, PlanningFieldValues, PlanningItem,
} from '../../../core/models';
import { PlanningService } from '../../../core/services/planning.service';
import { PlanningCardUpdate, PlanningGateway } from './planning.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyPlanningGateway implements PlanningGateway {
  private readonly planningService = inject(PlanningService);

  list(universeId: string): Promise<PlanningItem[]> {
    return this.planningService.list(universeId);
  }

  create(universeId: string, title: string, description: string, chapterId: string | null, image: string): Promise<string> {
    return this.planningService.create(universeId, title, description, chapterId, image);
  }

  saveCard(id: string, universeId: string, update: PlanningCardUpdate): Promise<void> {
    return this.planningService.saveCard(id, universeId, update);
  }

  saveOrder(universeId: string, items: PlanningItem[]): Promise<void> {
    return this.planningService.saveOrder(universeId, items);
  }

  delete(id: string, universeId: string): Promise<void> {
    return this.planningService.delete(id, universeId);
  }

  listFieldLinks(cardId: string): Promise<PlanningFieldValues> {
    return this.planningService.listFieldLinks(cardId);
  }

  listFieldDefinitions(universeId: string): Promise<PlanningFieldDefinition[]> {
    return this.planningService.listFieldDefinitions(universeId);
  }

  createFieldDefinition(universeId: string, name: string, fieldType: PlanningFieldType, options: string[]): Promise<PlanningFieldDefinition> {
    return this.planningService.createFieldDefinition(universeId, name, fieldType, options);
  }

  renameFieldDefinition(id: string, universeId: string, name: string): Promise<void> {
    return this.planningService.renameFieldDefinition(id, universeId, name);
  }

  deleteFieldDefinition(id: string, universeId: string): Promise<void> {
    return this.planningService.deleteFieldDefinition(id, universeId);
  }
}
