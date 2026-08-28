// ============================================
// NarraHub — App State (Angular Signals)
// ============================================

import { Injectable, signal, computed } from '@angular/core';
import { UniverseWithStats } from '../models';

export type AppView = 'home' | 'workspace';

/**
 * Estado que sobrevive à navegação, NÃO a navegação em si.
 *
 * `workspaceView` foi removido na Fase 3: qual seção está aberta é a URL, e
 * manter uma cópia disso aqui criava uma segunda fonte de verdade que já
 * causou tela em branco quando as duas discordavam. Se precisar saber a seção
 * ativa, leia `AppNavigationService.activeData()`, derivado de `route.data`.
 */
@Injectable({ providedIn: 'root' })
export class AppState {
  // ── Universo ativo ──────────────────────────
  readonly activeUniverse = signal<UniverseWithStats | null>(null);
  readonly activeUniverseId = computed(() => this.activeUniverse()?.id ?? null);
  /** Biblioteca vs. workspace — usado por busca global e pelo shell, não para escolher página. */
  readonly currentView = signal<AppView>('home');

  // ── UI ──────────────────────────────────────
  readonly sidebarCollapsed = signal(false);
  readonly isLoading = signal(false);
  readonly modalOpen = signal<string | null>(null);

  openUniverse(universe: UniverseWithStats): void {
    this.activeUniverse.set(universe);
    this.currentView.set('workspace');
  }

  goHome(): void {
    this.currentView.set('home');
    this.activeUniverse.set(null);
  }

  toggleSidebar(): void {
    this.sidebarCollapsed.update((collapsed) => !collapsed);
  }

  openModal(modalId: string): void {
    this.modalOpen.set(modalId);
  }

  closeModal(): void {
    this.modalOpen.set(null);
  }
}
