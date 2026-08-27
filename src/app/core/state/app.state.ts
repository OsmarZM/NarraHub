// ============================================
// NarraHub — App State (Angular Signals)
// ============================================

import { Injectable, signal, computed } from '@angular/core';
import { UniverseWithStats } from '../models';

export type AppView = 'home' | 'workspace';
export type WorkspaceView = 'editor' | 'entities' | 'entity-sheet' | 'graph' | 'timeline' | 'planning' | 'history' | 'settings';

@Injectable({ providedIn: 'root' })
export class AppState {
  // ── Navigation ──────────────────────────────
  readonly currentView = signal<AppView>('home');
  readonly workspaceView = signal<WorkspaceView>('editor');

  // ── Active Universe ─────────────────────────
  readonly activeUniverse = signal<UniverseWithStats | null>(null);
  readonly activeUniverseId = computed(() => this.activeUniverse()?.id ?? null);

  // ── Sidebar ─────────────────────────────────
  readonly sidebarCollapsed = signal(false);

  // ── UI State ────────────────────────────────
  readonly isLoading = signal(false);
  readonly modalOpen = signal<string | null>(null); // modal identifier

  // ── Methods ─────────────────────────────────

  openUniverse(universe: UniverseWithStats): void {
    this.activeUniverse.set(universe);
    this.currentView.set('workspace');
    this.workspaceView.set('editor');
  }

  goHome(): void {
    this.currentView.set('home');
    this.activeUniverse.set(null);
  }

  openEntityList(): void {
    this.workspaceView.set('entities');
  }

  openEntitySheet(): void {
    this.workspaceView.set('entity-sheet');
  }

  openEditor(): void {
    this.workspaceView.set('editor');
  }

  openGraph(): void {
    this.workspaceView.set('graph');
  }

  openTimeline(): void {
    this.workspaceView.set('timeline');
  }

  openPlanning(): void {
    this.workspaceView.set('planning');
  }

  openHistory(): void {
    this.workspaceView.set('history');
  }

  openSettings(): void {
    this.workspaceView.set('settings');
  }

  toggleSidebar(): void {
    this.sidebarCollapsed.update(v => !v);
  }

  openModal(modalId: string): void {
    this.modalOpen.set(modalId);
  }

  closeModal(): void {
    this.modalOpen.set(null);
  }
}
