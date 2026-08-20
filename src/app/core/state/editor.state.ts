// ============================================
// NarraHub — Editor State (Angular Signals)
// ============================================

import { Injectable, signal, computed } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class EditorState {
  // ── Editor Modes ────────────────────────────
  readonly focusMode = signal(false);
  readonly fullscreenMode = signal(false);
  readonly isWriting = signal(false);

  // ── Content ─────────────────────────────────
  readonly wordCount = signal(0);
  readonly isSaving = signal(false);
  readonly lastSaved = signal<Date | null>(null);

  // ── Status display ──────────────────────────
  readonly saveStatus = computed(() => {
    if (this.isSaving()) return 'Salvando...';
    if (this.lastSaved()) return 'Salvo';
    return '';
  });

  // ── Methods ─────────────────────────────────

  toggleFocusMode(): void {
    this.focusMode.update(v => !v);
  }

  toggleFullscreen(): void {
    this.fullscreenMode.update(v => !v);
  }

  setSaving(saving: boolean): void {
    this.isSaving.set(saving);
    if (!saving) {
      this.lastSaved.set(new Date());
    }
  }

  setWordCount(count: number): void {
    this.wordCount.set(count);
  }
}
