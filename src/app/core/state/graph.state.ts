// ============================================
// NarraHub — Graph State (Angular Signals)
// ============================================

import { Injectable, signal, computed } from '@angular/core';
import { GraphViewMode, GraphFilters } from '../models';

@Injectable({ providedIn: 'root' })
export class GraphState {
  // ── View Mode ───────────────────────────────
  readonly viewMode = signal<GraphViewMode>('misto');

  // ── Filters ─────────────────────────────────
  readonly activeEntityTypes = signal<Set<string>>(
    new Set(['Personagem', 'Lugar', 'Evento', 'Objeto', 'Organização'])
  );
  readonly focalEntityId = signal<string | null>(null);
  readonly depth = signal(3);
  readonly showCanonOnly = signal(false);

  // ── Selected Node ───────────────────────────
  readonly selectedNodeId = signal<string | null>(null);

  // ── Computed Filters ────────────────────────
  readonly filters = computed<GraphFilters>(() => ({
    entityTypes: this.activeEntityTypes(),
    focalEntityId: this.focalEntityId(),
    depth: this.depth(),
    showCanonOnly: this.showCanonOnly(),
  }));

  // ── Methods ─────────────────────────────────

  setViewMode(mode: GraphViewMode): void {
    this.viewMode.set(mode);
  }

  toggleEntityType(type: string): void {
    this.activeEntityTypes.update(types => {
      const newTypes = new Set(types);
      if (newTypes.has(type)) {
        newTypes.delete(type);
      } else {
        newTypes.add(type);
      }
      return newTypes;
    });
  }

  setFocalEntity(entityId: string | null): void {
    this.focalEntityId.set(entityId);
  }

  setDepth(depth: number): void {
    this.depth.set(depth);
  }

  selectNode(nodeId: string | null): void {
    this.selectedNodeId.set(nodeId);
  }

  resetFilters(): void {
    this.activeEntityTypes.set(new Set(['Personagem', 'Lugar', 'Evento', 'Objeto', 'Organização']));
    this.focalEntityId.set(null);
    this.depth.set(3);
    this.showCanonOnly.set(false);
  }
}
