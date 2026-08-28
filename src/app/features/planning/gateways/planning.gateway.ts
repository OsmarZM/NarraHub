import {
  PlanningFieldDefinition, PlanningFieldType, PlanningFieldValues, PlanningItem,
} from '../../../core/models';
import { PlanningStatus } from '../../../core/models';

/**
 * O card inteiro como a tela o edita. Morava em `PlanningService`, junto do
 * SQL que a Fase 4 levou para o Rust.
 */
export interface PlanningCardUpdate {
  title: string;
  description: string;
  image: string;
  status: PlanningStatus;
  chapterId: string | null;
  fieldValues: PlanningFieldValues;
}

export abstract class PlanningGateway {
  abstract list(universeId: string): Promise<PlanningItem[]>;
  abstract create(universeId: string, title: string, description: string, chapterId: string | null, image: string): Promise<string>;
  abstract saveCard(id: string, universeId: string, update: PlanningCardUpdate): Promise<void>;
  abstract saveOrder(universeId: string, items: PlanningItem[]): Promise<void>;
  abstract delete(id: string, universeId: string): Promise<void>;

  abstract listFieldLinks(cardId: string): Promise<PlanningFieldValues>;
  abstract listFieldDefinitions(universeId: string): Promise<PlanningFieldDefinition[]>;
  abstract createFieldDefinition(universeId: string, name: string, fieldType: PlanningFieldType, options: string[]): Promise<PlanningFieldDefinition>;
  abstract renameFieldDefinition(id: string, universeId: string, name: string): Promise<void>;
  abstract deleteFieldDefinition(id: string, universeId: string): Promise<void>;
}
