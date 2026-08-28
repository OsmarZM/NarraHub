import { Injectable, inject } from '@angular/core';
import { Attachment, Entity, EntityAttribute, EntityWithDetails } from '../../../core/models';
import { AttachmentService } from '../../../core/services/attachment.service';
import { EntityService } from '../../../core/services/entity.service';
import { CreateEntityInput, EntityGateway, UpdateEntityInput } from './entity.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyEntityGateway implements EntityGateway {
  private readonly entityService = inject(EntityService);
  private readonly attachmentService = inject(AttachmentService);

  list(universeId: string): Promise<Entity[]> {
    return this.entityService.listByUniverse(universeId);
  }

  getWithDetails(entityId: string): Promise<EntityWithDetails | null> {
    return this.entityService.getWithDetails(entityId);
  }

  async create(input: CreateEntityInput): Promise<Entity> {
    const entity = await this.entityService.create(input.universeId, input.type, input.name, input.description);
    if (input.image) await this.entityService.update(entity.id, { image: input.image });
    for (const attribute of input.attributes ?? []) {
      const key = attribute.key.trim();
      if (key) await this.entityService.setAttribute(entity.id, key, attribute.value.trim());
    }
    return entity;
  }

  update(entityId: string, patch: UpdateEntityInput): Promise<void> {
    // O contrato é camelCase; o serviço legado fala em nome de coluna.
    return this.entityService.update(entityId, {
      name: patch.name,
      description: patch.description,
      summary: patch.summary,
      image: patch.image,
      canon_status: patch.canonStatus,
    });
  }

  delete(entityId: string): Promise<void> {
    return this.entityService.delete(entityId);
  }

  saveAttribute(attribute: EntityAttribute): Promise<void> {
    return this.entityService.saveAttribute(attribute);
  }

  removeAttribute(attributeId: string): Promise<void> {
    return this.entityService.removeAttribute(attributeId);
  }

  listGallery(universeId: string, entityId: string): Promise<Attachment[]> {
    return this.attachmentService.list(universeId, 'entity', entityId);
  }

  createGalleryImage(universeId: string, entityId: string, dataUrl: string, caption: string): Promise<Attachment> {
    return this.attachmentService.create(universeId, 'entity', entityId, dataUrl, caption);
  }

  async deleteGalleryImage(attachmentId: string): Promise<void> {
    await this.attachmentService.delete(attachmentId);
  }
}
