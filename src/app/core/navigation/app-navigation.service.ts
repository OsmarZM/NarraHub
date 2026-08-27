import { Injectable, inject, signal } from '@angular/core';
import { ActivatedRouteSnapshot, NavigationEnd, Route, Router, RoutesRecognized } from '@angular/router';
import { filter } from 'rxjs';
import { AppNavigationData, AppNavigationId, AppRouteState, buildAppPath, parseAppPath } from './app-navigation';

const defaultNavigationData: AppNavigationData = {
  navigationId: 'inicio',
  label: 'Universos',
  sidebarLabel: 'Início',
  icon: '⌂',
  needsUniverse: false,
  order: 0,
};

@Injectable({ providedIn: 'root' })
export class AppNavigationService {
  private readonly router = inject(Router);
  readonly route = signal<AppRouteState>(parseAppPath(this.router.url));
  readonly activeData = signal<AppNavigationData>(defaultNavigationData);
  readonly navigationItems = this.collectNavigationItems(this.router.config);

  constructor() {
    this.router.events.pipe(filter((event): event is RoutesRecognized => event instanceof RoutesRecognized))
      .subscribe((event) => this.updateRouteState(event.urlAfterRedirects, event.state.root));
    this.router.events.pipe(filter((event): event is NavigationEnd => event instanceof NavigationEnd))
      .subscribe((event) => this.updateRouteState(event.urlAfterRedirects, this.router.routerState.snapshot.root));
  }

  async navigate(navId: AppNavigationId, universeId: string | null): Promise<void> {
    const path = buildAppPath(navId, universeId);
    if (this.router.url !== path) await this.router.navigateByUrl(path);
  }

  private updateRouteState(url: string, snapshot: ActivatedRouteSnapshot): void {
    const parsed = parseAppPath(url);
    const data = this.findActiveNavigationData(snapshot) ?? defaultNavigationData;
    this.route.set({ navId: data.navigationId, universeId: parsed.universeId });
    this.activeData.set(data);
  }

  private findActiveNavigationData(snapshot: ActivatedRouteSnapshot): AppNavigationData | null {
    let current: ActivatedRouteSnapshot | undefined = snapshot;
    let found: AppNavigationData | null = null;
    while (current) {
      if (this.isNavigationData(current.data)) found = current.data;
      current = current.firstChild ?? undefined;
    }
    return found;
  }

  private collectNavigationItems(routes: readonly Route[]): AppNavigationData[] {
    const items: AppNavigationData[] = [];
    const visit = (route: Route): void => {
      if (this.isNavigationData(route.data)) items.push(route.data);
      route.children?.forEach(visit);
    };
    routes.forEach(visit);
    return items.sort((left, right) => left.order - right.order);
  }

  private isNavigationData(
    data: Record<string, unknown> | undefined,
  ): data is Record<string, unknown> & AppNavigationData {
    return !!data
      && typeof data['navigationId'] === 'string'
      && typeof data['label'] === 'string'
      && typeof data['icon'] === 'string'
      && typeof data['needsUniverse'] === 'boolean'
      && typeof data['order'] === 'number';
  }
}
