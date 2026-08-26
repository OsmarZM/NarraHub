import { Routes } from '@angular/router';

export const routes: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'library' },
  { path: 'library', children: [] },
  { path: 'settings', children: [] },
  { path: 'workspace/:universeId', pathMatch: 'full', redirectTo: 'workspace/:universeId/writing' },
  { path: 'workspace/:universeId/:section', children: [] },
  { path: '**', redirectTo: 'library' },
];
