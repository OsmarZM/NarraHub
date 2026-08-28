import { ApplicationConfig, inject, provideAppInitializer, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter, withComponentInputBinding, withRouterConfig } from '@angular/router';
import { routes } from './app.routes';
import { AppBootstrapService } from './bootstrap/app-bootstrap.service';
import { CollaborationGateway } from './features/collaboration/gateways/collaboration.gateway';
import { RustCollaborationGateway } from './features/collaboration/gateways/rust-collaboration.gateway';
import { ConnectionsGateway } from './features/connections/gateways/connections.gateway';
import { RustConnectionsGateway } from './features/connections/gateways/rust-connections.gateway';
import { EntityGateway } from './features/entities/gateways/entity.gateway';
import { RustEntityGateway } from './features/entities/gateways/rust-entity.gateway';
import { HistoryGateway } from './features/history/gateways/history.gateway';
import { RustHistoryGateway } from './features/history/gateways/rust-history.gateway';
import { KnowledgeGateway } from './features/knowledge/gateways/knowledge.gateway';
import { RustKnowledgeGateway } from './features/knowledge/gateways/rust-knowledge.gateway';
import { UniverseGateway } from './features/library/gateways/universe.gateway';
import { RustUniverseGateway } from './features/library/gateways/rust-universe.gateway';
import { PlanningGateway } from './features/planning/gateways/planning.gateway';
import { RustPlanningGateway } from './features/planning/gateways/rust-planning.gateway';
import { ManuscriptGateway } from './features/manuscript/gateways/manuscript.gateway';
import { RustManuscriptGateway } from './features/manuscript/gateways/rust-manuscript.gateway';
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
    // migrado; quem aponta para Rust* pode ainda delegar métodos ao legado
    // por dentro — a lista de delegações está documentada em cada adaptador.
    { provide: CollaborationGateway, useExisting: RustCollaborationGateway },
    { provide: ConnectionsGateway, useExisting: RustConnectionsGateway },
    { provide: EntityGateway, useExisting: RustEntityGateway },
    { provide: HistoryGateway, useExisting: RustHistoryGateway },
    { provide: KnowledgeGateway, useExisting: RustKnowledgeGateway },
    { provide: ManuscriptGateway, useExisting: RustManuscriptGateway },
    { provide: PlanningGateway, useExisting: RustPlanningGateway },
    { provide: TimelineGateway, useExisting: RustTimelineGateway },
    { provide: UniverseGateway, useExisting: RustUniverseGateway },
  ]
};
