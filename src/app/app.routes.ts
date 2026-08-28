import { Routes } from '@angular/router';
import { AppNavigationData, AppNavigationId } from './core/navigation/app-navigation';
import { unsavedChapterGuard } from './routing/unsaved-chapter.guard';
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
  {
    path: 'writing',
    data: navigationData('escrita', 'Escrita', '✎', true, 10),
    loadComponent: () => import('./features/manuscript/writing-page.component').then((module) => module.WritingPageComponent),
    canDeactivate: [unsavedChapterGuard],
  },
  {
    // Deep link para um capítulo. O Angular não tem parâmetro opcional, então a
    // variante com :chapterId é uma rota própria — oculta do menu para não
    // duplicar "Escrita" na sidebar. Id inexistente cai na seleção padrão em vez
    // de erro: a URL pode apontar para um capítulo já excluído.
    path: 'writing/:chapterId',
    data: { ...navigationData('escrita', 'Escrita', '✎', true, 10), hiddenFromMenu: true },
    loadComponent: () => import('./features/manuscript/writing-page.component').then((module) => module.WritingPageComponent),
    canDeactivate: [unsavedChapterGuard],
  },
  {
    path: 'entities',
    data: navigationData('entidades', 'Entidades', '♧', true, 20),
    loadComponent: () => import('./features/entities/entities-page/entities-page.component').then((module) => module.EntitiesPageComponent),
  },
  {
    path: 'entities/:entityId',
    data: { ...navigationData('entidades', 'Entidades', '♧', true, 20), hiddenFromMenu: true },
    loadComponent: () => import('./features/entities/entities-page/entities-page.component').then((module) => module.EntitiesPageComponent),
  },
  {
    path: 'connections',
    data: navigationData('conexoes', 'Conexões', '⌘', true, 30),
    loadComponent: () => import('./features/connections/connections-page.component').then((module) => module.ConnectionsPageComponent),
  },
  {
    path: 'timeline',
    data: navigationData('timeline', 'Timeline', '◷', true, 40),
    loadComponent: () => import('./features/timeline/timeline-page.component').then((module) => module.TimelinePageComponent),
  },
  {
    path: 'planning',
    data: navigationData('planejamento', 'Planejamento', '☑', true, 50),
    loadComponent: () => import('./features/planning/planning-board.component').then((module) => module.PlanningBoardComponent),
  },
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
