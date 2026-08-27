import { Attachment, Entity, EntityAttribute, EntityType, EntityWithDetails } from '../../../core/models';

export interface CreateEntityInput {
  universeId: string;
  type: EntityType;
  name: string;
  description: string;
  image?: string;
  attributes?: Array<{ key: string; value: string }>;
}

export type UpdateEntityInput = Partial<Pick<Entity, 'name' | 'description' | 'summary' | 'image' | 'canon_status'>>;

export abstract class EntityGateway {
  abstract list(universeId: string): Promise<Entity[]>;
  abstract getWithDetails(entityId: string): Promise<EntityWithDetails | null>;
  abstract create(input: CreateEntityInput): Promise<Entity>;
  abstract update(entityId: string, patch: UpdateEntityInput): Promise<void>;
  abstract delete(entityId: string): Promise<void>;
  abstract saveAttribute(attribute: EntityAttribute): Promise<void>;
  abstract removeAttribute(attributeId: string): Promise<void>;
  abstract listGallery(universeId: string, entityId: string): Promise<Attachment[]>;
  abstract createGalleryImage(universeId: string, entityId: string, dataUrl: string, caption: string): Promise<Attachment>;
  abstract deleteGalleryImage(attachmentId: string): Promise<void>;
}
