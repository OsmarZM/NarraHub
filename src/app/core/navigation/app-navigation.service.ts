import { Injectable, inject, signal } from '@angular/core';
import { NavigationEnd, Router } from '@angular/router';
import { filter } from 'rxjs';
import { AppNavigationId, AppRouteState, buildAppPath, parseAppPath } from './app-navigation';

@Injectable({ providedIn: 'root' })
export class AppNavigationService {
  private readonly router = inject(Router);
  readonly route = signal<AppRouteState>(parseAppPath(this.router.url));

  constructor() {
    this.router.events.pipe(filter((event): event is NavigationEnd => event instanceof NavigationEnd))
      .subscribe((event) => this.route.set(parseAppPath(event.urlAfterRedirects)));
  }

  async navigate(navId: AppNavigationId, universeId: string | null): Promise<void> {
    const path = buildAppPath(navId, universeId);
    if (this.router.url !== path) await this.router.navigateByUrl(path);
  }
}

