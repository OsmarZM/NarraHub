import { RelationCard } from '../../../core/models';

export abstract class ConnectionsGateway {
  abstract listRelations(universeId: string): Promise<RelationCard[]>;
  abstract createRelation(universeId: string, sourceId: string, targetId: string, label: string): Promise<void>;
  abstract deleteRelation(id: string): Promise<void>;
}
