import { Injectable, inject } from '@angular/core';
import { Attachment, Entity, EntityAttribute, EntityWithDetails } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { CreateEntityInput, EntityGateway, UpdateEntityInput } from './entity.gateway';
import { LegacyEntityGateway } from './legacy-entity.gateway';

/**
 * Ordem 4 — entidades e atributos.
 *
 * A galeria continua no legado: anexo é `AttachmentService`, compartilhado com
 * capítulo e universo, e não está na ordem de migração do plano. Ele sai do
 * legado junto do último domínio que o usa, para não ficar meio migrado.
 */
@Injectable({ providedIn: 'root' })
export class RustEntityGateway implements EntityGateway {
  private readonly core = inject(RustCoreService);
  private readonly legacy = inject(LegacyEntityGateway);

  list(universeId: string): Promise<Entity[]> {
    return this.core.call<Entity[]>('entity_list', { universeId });
  }

  getWithDetails(entityId: string): Promise<EntityWithDetails | null> {
    return this.core.call<EntityWithDetails | null>('entity_details', { id: entityId });
  }

  create(input: CreateEntityInput): Promise<Entity> {
    // A ficha inteira vai numa chamada só: o core monta entidade, atributos
    // padrão, templates do universo e o que veio do formulário na mesma
    // transação. O caminho antigo eram N gravações soltas, e uma falha no
    // meio deixava a entidade pela metade no arquivo.
    return this.core.call<Entity>('entity_create', {
      input: {
        universeId: input.universeId,
        type: input.type,
        name: input.name,
        description: input.description,
        image: input.image ?? '',
        attributes: input.attributes ?? [],
      },
    });
  }

  update(entityId: string, patch: UpdateEntityInput): Promise<void> {
    return this.core.call<void>('entity_update', { id: entityId, patch });
  }

  delete(entityId: string): Promise<void> {
    return this.core.call<void>('entity_delete', { id: entityId });
  }

  saveAttribute(attribute: EntityAttribute): Promise<void> {
    return this.core.call<void>('entity_attribute_save', { attribute });
  }

  removeAttribute(attributeId: string): Promise<void> {
    return this.core.call<void>('entity_attribute_delete', { id: attributeId });
  }

  listGallery(universeId: string, entityId: string): Promise<Attachment[]> {
    return this.legacy.listGallery(universeId, entityId);
  }

  createGalleryImage(universeId: string, entityId: string, dataUrl: string, caption: string): Promise<Attachment> {
    return this.legacy.createGalleryImage(universeId, entityId, dataUrl, caption);
  }

  deleteGalleryImage(attachmentId: string): Promise<void> {
    return this.legacy.deleteGalleryImage(attachmentId);
  }
}
