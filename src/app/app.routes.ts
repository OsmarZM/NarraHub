import { Routes } from '@angular/router';
import { AppNavigationData, AppNavigationId } from './core/navigation/app-navigation';
import { universeResolver } from './routing/universe.resolver';

const navigationData = (
  navigationId: AppNavigationId,
  label: string,
  icon: string,
  needsUniverse: boolean,
  order: number,
  sidebarLabel?: string,
): AppNavigationData => ({ navigationId, label, icon, needsUniverse, order, sidebarLabel });

const workspaceSections: Routes = [
  { path: 'writing', data: navigationData('escrita', 'Escrita', '✎', true, 10), children: [] },
  { path: 'entities', data: navigationData('entidades', 'Entidades', '♧', true, 20), children: [] },
  { path: 'connections', data: navigationData('conexoes', 'Conexões', '⌘', true, 30), children: [] },
  { path: 'timeline', data: navigationData('timeline', 'Timeline', '◷', true, 40), children: [] },
  { path: 'planning', data: navigationData('planejamento', 'Planejamento', '☑', true, 50), children: [] },
  {
    path: 'history',
    data: navigationData('historico', 'Histórico', '↶', true, 60),
    loadComponent: () => import('./features/history/history-page.component').then((module) => module.HistoryPageComponent),
  },
];

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
        data: navigationData('inicio', 'Universos', '⌂', false, 0, 'Início'),
      },
      {
        path: 'settings',
        loadComponent: () => import('./features/settings/settings-page.component').then((module) => module.SettingsPageComponent),
        data: navigationData('configuracoes', 'Configurações', '⚙', false, 70),
      },
      {
        path: 'workspace/:universeId',
        loadComponent: () => import('./workspace-layout.component').then((module) => module.WorkspaceLayoutComponent),
        data: { navigationId: 'workspace', label: 'Workspace' },
        resolve: { universe: universeResolver },
        children: [
          { path: '', pathMatch: 'full', redirectTo: 'writing' },
          ...workspaceSections,
          { path: '**', redirectTo: 'writing' },
        ],
      },
    ],
  },
  { path: '**', redirectTo: 'library' },
];
