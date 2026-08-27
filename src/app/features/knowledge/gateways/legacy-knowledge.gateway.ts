import { Injectable, inject } from '@angular/core';
import { ContentTag, ContentTagAssignment, MentionOccurrence, MetadataOwnerType } from '../../../core/models';
import { MentionService } from '../../../core/services/mention.service';
import { MetadataService } from '../../../core/services/metadata.service';
import { KnowledgeGateway } from './knowledge.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyKnowledgeGateway implements KnowledgeGateway {
  private readonly metadataService = inject(MetadataService);
  private readonly mentionService = inject(MentionService);

  listTags(universeId: string): Promise<ContentTag[]> { return this.metadataService.listTags(universeId); }
  listOwnerTags(ownerType: MetadataOwnerType, ownerId: string): Promise<ContentTag[]> { return this.metadataService.listOwnerTags(ownerType, ownerId); }
  listTagAssignments(universeIds: string[], ownerTypes?: MetadataOwnerType[]): Promise<ContentTagAssignment[]> { return this.metadataService.listAssignments(universeIds, ownerTypes); }
  createTag(universeId: string, name: string, color: string): Promise<ContentTag> { return this.metadataService.createTag(universeId, name, color); }
  setTag(ownerType: MetadataOwnerType, ownerId: string, tagId: string, assigned: boolean): Promise<void> { return this.metadataService.setTag(ownerType, ownerId, tagId, assigned); }
  async deleteTag(id: string): Promise<void> { await this.metadataService.deleteTag(id); }

  listMentionsByUniverse(universeId: string): Promise<MentionOccurrence[]> { return this.mentionService.listByUniverse(universeId); }
  syncChapterMentions(chapterId: string, entityIds: string[]): Promise<void> { return this.mentionService.syncChapterMentions(chapterId, entityIds); }
}
