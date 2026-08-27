import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter } from '@angular/router';
import { routes } from './app.routes';
import { CollaborationGateway } from './features/collaboration/gateways/collaboration.gateway';
import { LegacyCollaborationGateway } from './features/collaboration/gateways/legacy-collaboration.gateway';
import { EntityGateway } from './features/entities/gateways/entity.gateway';
import { LegacyEntityGateway } from './features/entities/gateways/legacy-entity.gateway';
import { HistoryGateway } from './features/history/gateways/history.gateway';
import { LegacyHistoryGateway } from './features/history/gateways/legacy-history.gateway';
import { LegacyUniverseGateway } from './features/library/gateways/legacy-universe.gateway';
import { UniverseGateway } from './features/library/gateways/universe.gateway';
import { LegacyManuscriptGateway } from './features/manuscript/gateways/legacy-manuscript.gateway';
import { ManuscriptGateway } from './features/manuscript/gateways/manuscript.gateway';
import { LegacyTimelineGateway } from './features/timeline/gateways/legacy-timeline.gateway';
import { TimelineGateway } from './features/timeline/gateways/timeline.gateway';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes),
    { provide: CollaborationGateway, useExisting: LegacyCollaborationGateway },
    { provide: EntityGateway, useExisting: LegacyEntityGateway },
    { provide: HistoryGateway, useExisting: LegacyHistoryGateway },
    { provide: ManuscriptGateway, useExisting: LegacyManuscriptGateway },
    { provide: TimelineGateway, useExisting: LegacyTimelineGateway },
    { provide: UniverseGateway, useExisting: LegacyUniverseGateway },
  ]
};
