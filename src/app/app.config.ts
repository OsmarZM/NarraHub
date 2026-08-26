import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter } from '@angular/router';
import { routes } from './app.routes';
import { HistoryGateway } from './features/history/gateways/history.gateway';
import { LegacyHistoryGateway } from './features/history/gateways/legacy-history.gateway';
import { LegacyTimelineGateway } from './features/timeline/gateways/legacy-timeline.gateway';
import { TimelineGateway } from './features/timeline/gateways/timeline.gateway';

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes),
    { provide: HistoryGateway, useExisting: LegacyHistoryGateway },
    { provide: TimelineGateway, useExisting: LegacyTimelineGateway },
  ]
};
