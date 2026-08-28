import { Attachment, CanonStatus, Entity, EntityAttribute, EntityType, EntityWithDetails } from '../../../core/models';

export interface CreateEntityInput {
  universeId: string;
  type: EntityType;
  name: string;
  description: string;
  image?: string;
  attributes?: Array<{ key: string; value: string }>;
}

/**
 * `camelCase`, como `UpdateUniverseInput` e `UpdateBookInput` — e **não** um
 * `Pick<Entity>`, que traria `canon_status` do modelo.
 *
 * O comando Rust desserializa este objeto num struct `rename_all =
 * "camelCase"`: uma chave em `snake_case` não casa com campo nenhum, o serde a
 * ignora em silêncio e o comando devolve sucesso sem ter gravado. Foi
 * exatamente o que aconteceu com `canon_status` — a tela mostrava o novo
 * estado, o banco guardava o antigo, e nada acusava. Declarar o contrato aqui
 * faz o compilador apontar o chamador, em vez de o erro aparecer só quando o
 * usuário reabre a ficha.
 */
export interface UpdateEntityInput {
  name?: string;
  description?: string;
  summary?: string;
  image?: string;
  canonStatus?: CanonStatus;
}

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
