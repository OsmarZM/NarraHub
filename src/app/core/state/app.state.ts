// ============================================
// NarraHub — App State (Angular Signals)
// ============================================

import { Injectable, signal, computed } from '@angular/core';
import { UniverseWithStats } from '../models';

export type AppView = 'home' | 'workspace';
export type WorkspaceView = 'editor' | 'entities' | 'entity-sheet' | 'graph' | 'timeline' | 'planning' | 'history' | 'settings';
export type EntityListFilter = string | null; // entity type or null for all

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
  readonly sidebarEntityFilter = signal<EntityListFilter>(null);

  // ── Active Selection ────────────────────────
  readonly activeStoryId = signal<string | null>(null);
  readonly activeBookId = signal<string | null>(null);
  readonly activeChapterId = signal<string | null>(null);
  readonly activeEntityId = signal<string | null>(null);

  // ── UI State ────────────────────────────────
  readonly isLoading = signal(false);
  readonly modalOpen = signal<string | null>(null); // modal identifier

  // ── Methods ─────────────────────────────────

  openUniverse(universe: UniverseWithStats): void {
    this.activeUniverse.set(universe);
    this.currentView.set('workspace');
    this.workspaceView.set('editor');
    this.activeStoryId.set(null);
    this.activeBookId.set(null);
    this.activeChapterId.set(null);
    this.activeEntityId.set(null);
  }

  goHome(): void {
    this.currentView.set('home');
    this.activeUniverse.set(null);
    this.activeStoryId.set(null);
    this.activeBookId.set(null);
    this.activeChapterId.set(null);
    this.activeEntityId.set(null);
  }

  openEntityList(type: string | null = null): void {
    this.workspaceView.set('entities');
    this.sidebarEntityFilter.set(type);
    this.activeEntityId.set(null);
  }

  openEntitySheet(entityId: string): void {
    this.workspaceView.set('entity-sheet');
    this.activeEntityId.set(entityId);
  }

  openEditor(chapterId?: string): void {
    this.workspaceView.set('editor');
    if (chapterId) {
      this.activeChapterId.set(chapterId);
    }
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
