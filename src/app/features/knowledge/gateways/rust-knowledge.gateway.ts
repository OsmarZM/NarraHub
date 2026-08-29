import { Injectable, inject } from '@angular/core';
import { ContentTag, ContentTagAssignment, MentionOccurrence, MetadataOwnerType } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { KnowledgeGateway } from './knowledge.gateway';

/**
 * Ordem 4 (tags) e Ordem 5 (menções) — domínio migrado por inteiro.
 */
@Injectable({ providedIn: 'root' })
export class RustKnowledgeGateway implements KnowledgeGateway {
  private readonly core = inject(RustCoreService);

  listTags(universeId: string): Promise<ContentTag[]> {
    return this.core.call<ContentTag[]>('tags_list', { universeId });
  }

  listOwnerTags(ownerType: MetadataOwnerType, ownerId: string): Promise<ContentTag[]> {
    return this.core.call<ContentTag[]>('tags_for_owner', { ownerType, ownerId });
  }

  listTagAssignments(universeIds: string[], ownerTypes?: MetadataOwnerType[]): Promise<ContentTagAssignment[]> {
    // Lista vazia em vez de `undefined`: o comando espera um vetor, e
    // "sem filtro" é justamente o vetor vazio do lado do Rust.
    return this.core.call<ContentTagAssignment[]>('tag_assignments', {
      universeIds, ownerTypes: ownerTypes ?? [],
    });
  }

  createTag(universeId: string, name: string, color: string): Promise<ContentTag> {
    return this.core.call<ContentTag>('tag_create', { universeId, name, color });
  }

  setTag(ownerType: MetadataOwnerType, ownerId: string, tagId: string, assigned: boolean): Promise<void> {
    return this.core.call<void>('tag_set', { ownerType, ownerId, tagId, assigned });
  }

  deleteTag(id: string): Promise<void> {
    return this.core.call<void>('tag_delete', { id });
  }

  listMentionsByUniverse(universeId: string): Promise<MentionOccurrence[]> {
    return this.core.call<MentionOccurrence[]>('mentions_list', { universeId });
  }

  syncChapterMentions(chapterId: string, entityIds: string[]): Promise<void> {
    return this.core.call<void>('mentions_sync', { chapterId, entityIds });
  }
}
