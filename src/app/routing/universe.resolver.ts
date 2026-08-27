import { inject } from '@angular/core';
import { ActivatedRouteSnapshot, ResolveFn, Router, UrlTree } from '@angular/router';
import { AppBootstrapService } from '../bootstrap/app-bootstrap.service';
import { UniverseWithStats } from '../core/models';
import { AppState } from '../core/state/app.state';
import { UniverseStore } from '../features/library/state/universe.store';
import { ShellState } from '../shell/state/shell.state';

export const universeResolver: ResolveFn<UniverseWithStats | UrlTree> = async (
  route: ActivatedRouteSnapshot,
) => {
  const bootstrap = inject(AppBootstrapService);
  const router = inject(Router);
  const appState = inject(AppState);
  const shell = inject(ShellState);
  const universes = inject(UniverseStore);
  const universeId = route.paramMap.get('universeId')?.trim() ?? '';

  if (bootstrap.error()) {
    appState.goHome();
    return router.parseUrl('/library');
  }

  if (!universeId || universeId.length > 256) {
    appState.goHome();
    shell.showInfo('A rota do universo é inválida. Selecione um universo da biblioteca.');
    return router.parseUrl('/library');
  }

  let universe = universes.universes().find((item) => item.id === universeId) ?? null;
  if (!universe) {
    universes.clearError();
    await universes.load();
    universe = universes.universes().find((item) => item.id === universeId) ?? null;
  }

  if (!universe) {
    appState.goHome();
    if (universes.error()) {
      shell.showError('Não foi possível validar o universo desta rota.');
      return router.parseUrl('/library');
    }
    shell.showInfo('O universo desta rota não existe mais neste banco local.');
    return router.parseUrl('/library');
  }

  appState.openUniverse(universe);
  return universe;
};
