import { ApplicationConfig, inject, provideAppInitializer, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter, withComponentInputBinding, withRouterConfig } from '@angular/router';
import { routes } from './app.routes';
import { AppBootstrapService } from './bootstrap/app-bootstrap.service';
import { CollaborationGateway } from './features/collaboration/gateways/collaboration.gateway';
import { LegacyCollaborationGateway } from './features/collaboration/gateways/legacy-collaboration.gateway';
import { ConnectionsGateway } from './features/connections/gateways/connections.gateway';
import { LegacyConnectionsGateway } from './features/connections/gateways/legacy-connections.gateway';
import { EntityGateway } from './features/entities/gateways/entity.gateway';
import { LegacyEntityGateway } from './features/entities/gateways/legacy-entity.gateway';
import { HistoryGateway } from './features/history/gateways/history.gateway';
import { LegacyHistoryGateway } from './features/history/gateways/legacy-history.gateway';
import { KnowledgeGateway } from './features/knowledge/gateways/knowledge.gateway';
import { LegacyKnowledgeGateway } from './features/knowledge/gateways/legacy-knowledge.gateway';
import { LegacyUniverseGateway } from './features/library/gateways/legacy-universe.gateway';
import { UniverseGateway } from './features/library/gateways/universe.gateway';
import { LegacyManuscriptGateway } from './features/manuscript/gateways/legacy-manuscript.gateway';
import { LegacyPlanningGateway } from './features/planning/gateways/legacy-planning.gateway';
import { PlanningGateway } from './features/planning/gateways/planning.gateway';
import { ManuscriptGateway } from './features/manuscript/gateways/manuscript.gateway';
import { LegacyTimelineGateway } from './features/timeline/gateways/legacy-timeline.gateway';
import { TimelineGateway } from './features/timeline/gateways/timeline.gateway';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideAppInitializer(() => inject(AppBootstrapService).initialize()),
    provideRouter(
      routes,
      withComponentInputBinding(),
      withRouterConfig({ paramsInheritanceStrategy: 'always' }),
    ),
    { provide: CollaborationGateway, useExisting: LegacyCollaborationGateway },
    { provide: ConnectionsGateway, useExisting: LegacyConnectionsGateway },
    { provide: EntityGateway, useExisting: LegacyEntityGateway },
    { provide: HistoryGateway, useExisting: LegacyHistoryGateway },
    { provide: KnowledgeGateway, useExisting: LegacyKnowledgeGateway },
    { provide: ManuscriptGateway, useExisting: LegacyManuscriptGateway },
    { provide: PlanningGateway, useExisting: LegacyPlanningGateway },
    { provide: TimelineGateway, useExisting: LegacyTimelineGateway },
    { provide: UniverseGateway, useExisting: LegacyUniverseGateway },
  ]
};
