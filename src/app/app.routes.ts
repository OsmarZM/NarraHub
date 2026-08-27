import { Routes } from '@angular/router';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () => import('./root-layout.component').then((module) => module.RootLayoutComponent),
    data: { navigationId: 'root', label: 'NarraHub' },
    children: [
      { path: '', pathMatch: 'full', redirectTo: 'library' },
      {
        path: 'library',
        loadComponent: () => import('./features/library/library-route.component').then((module) => module.LibraryRouteComponent),
        data: { navigationId: 'inicio', label: 'Universos' },
      },
      {
        path: 'settings',
        loadComponent: () => import('./features/settings/settings-page.component').then((module) => module.SettingsPageComponent),
        data: { navigationId: 'configuracoes', label: 'Configurações' },
      },
      {
        path: 'workspace/:universeId',
        loadComponent: () => import('./workspace-layout.component').then((module) => module.WorkspaceLayoutComponent),
        data: { navigationId: 'workspace', label: 'Workspace' },
        children: [
          { path: '', pathMatch: 'full', redirectTo: 'writing' },
          { path: ':section', data: { navigationId: 'workspace-section', label: 'Workspace' }, children: [] },
        ],
      },
    ],
  },
  { path: '**', redirectTo: 'library' },
];
