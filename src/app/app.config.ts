import { ApplicationConfig, inject, provideAppInitializer, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter, withComponentInputBinding, withRouterConfig } from '@angular/router';
import { routes } from './app.routes';
import { AppBootstrapService } from './bootstrap/app-bootstrap.service';
import { CollaborationGateway } from './features/collaboration/gateways/collaboration.gateway';
import { LegacyCollaborationGateway } from './features/collaboration/gateways/legacy-collaboration.gateway';
import { ConnectionsGateway } from './features/connections/gateways/connections.gateway';
import { RustConnectionsGateway } from './features/connections/gateways/rust-connections.gateway';
import { LegacyConnectionsGateway } from './features/connections/gateways/legacy-connections.gateway';
import { EntityGateway } from './features/entities/gateways/entity.gateway';
import { RustEntityGateway } from './features/entities/gateways/rust-entity.gateway';
import { LegacyEntityGateway } from './features/entities/gateways/legacy-entity.gateway';
import { HistoryGateway } from './features/history/gateways/history.gateway';
import { RustHistoryGateway } from './features/history/gateways/rust-history.gateway';
import { LegacyHistoryGateway } from './features/history/gateways/legacy-history.gateway';
import { KnowledgeGateway } from './features/knowledge/gateways/knowledge.gateway';
import { RustKnowledgeGateway } from './features/knowledge/gateways/rust-knowledge.gateway';
import { LegacyKnowledgeGateway } from './features/knowledge/gateways/legacy-knowledge.gateway';
import { LegacyUniverseGateway } from './features/library/gateways/legacy-universe.gateway';
import { UniverseGateway } from './features/library/gateways/universe.gateway';
import { RustUniverseGateway } from './features/library/gateways/rust-universe.gateway';
import { LegacyManuscriptGateway } from './features/manuscript/gateways/legacy-manuscript.gateway';
import { LegacyPlanningGateway } from './features/planning/gateways/legacy-planning.gateway';
import { PlanningGateway } from './features/planning/gateways/planning.gateway';
import { RustPlanningGateway } from './features/planning/gateways/rust-planning.gateway';
import { ManuscriptGateway } from './features/manuscript/gateways/manuscript.gateway';
import { LegacyTimelineGateway } from './features/timeline/gateways/legacy-timeline.gateway';
import { TimelineGateway } from './features/timeline/gateways/timeline.gateway';
import { RustTimelineGateway } from './features/timeline/gateways/rust-timeline.gateway';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideAppInitializer(() => inject(AppBootstrapService).initialize()),
    provideRouter(
      routes,
      withComponentInputBinding(),
      withRouterConfig({ paramsInheritanceStrategy: 'always' }),
    ),
    // Fase 4: o adaptador Rust substitui o legado por domínio, conforme a
    // ordem de migração do plano. Quem ainda aponta para Legacy* não foi
    // migrado; quem aponta para Rust* pode ainda delegar métodos ao legado
    // por dentro — a lista de delegações está documentada em cada adaptador.
    { provide: CollaborationGateway, useExisting: LegacyCollaborationGateway },
    { provide: ConnectionsGateway, useExisting: RustConnectionsGateway },
    { provide: EntityGateway, useExisting: RustEntityGateway },
    { provide: HistoryGateway, useExisting: RustHistoryGateway },
    { provide: KnowledgeGateway, useExisting: RustKnowledgeGateway },
    { provide: ManuscriptGateway, useExisting: LegacyManuscriptGateway },
    { provide: PlanningGateway, useExisting: RustPlanningGateway },
    { provide: TimelineGateway, useExisting: RustTimelineGateway },
    { provide: UniverseGateway, useExisting: RustUniverseGateway },
  ]
};
