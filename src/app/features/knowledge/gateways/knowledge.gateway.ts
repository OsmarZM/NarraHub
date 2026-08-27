import { ContentTag, ContentTagAssignment, MentionOccurrence, MetadataOwnerType } from '../../../core/models';

export abstract class KnowledgeGateway {
  // Tags
  abstract listTags(universeId: string): Promise<ContentTag[]>;
  abstract listOwnerTags(ownerType: MetadataOwnerType, ownerId: string): Promise<ContentTag[]>;
  abstract listTagAssignments(universeIds: string[], ownerTypes?: MetadataOwnerType[]): Promise<ContentTagAssignment[]>;
  abstract createTag(universeId: string, name: string, color: string): Promise<ContentTag>;
  abstract setTag(ownerType: MetadataOwnerType, ownerId: string, tagId: string, assigned: boolean): Promise<void>;
  abstract deleteTag(id: string): Promise<void>;

  // Mentions
  abstract listMentionsByUniverse(universeId: string): Promise<MentionOccurrence[]>;
  abstract syncChapterMentions(chapterId: string, entityIds: string[]): Promise<void>;
}
