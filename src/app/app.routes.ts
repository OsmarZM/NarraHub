import { Routes } from '@angular/router';

export const routes: Routes = [
  {
    path: '',
    loadComponent: () => import('./root-layout.component').then((module) => module.RootLayoutComponent),
    data: { navigationId: 'root', label: 'NarraHub' },
    children: [
      { path: '', pathMatch: 'full', redirectTo: 'library' },
      { path: 'library', data: { navigationId: 'inicio', label: 'Universos' }, children: [] },
      {
        path: 'settings',
        loadComponent: () => import('./features/settings/settings-page.component').then((module) => module.SettingsPageComponent),
        data: { navigationId: 'configuracoes', label: 'Configurações' },
      },
      { path: 'workspace/:universeId', pathMatch: 'full', redirectTo: 'workspace/:universeId/writing' },
      { path: 'workspace/:universeId/:section', data: { navigationId: 'workspace', label: 'Workspace' }, children: [] },
    ],
  },
  { path: '**', redirectTo: 'library' },
];
