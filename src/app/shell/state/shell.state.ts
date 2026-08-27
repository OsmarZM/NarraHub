import { Injectable, signal } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class ShellState {
  readonly searchQuery = signal('');
  readonly focusMode = signal(false);
  readonly errorMessage = signal('');
  readonly infoMessage = signal('');

  private infoTimer: ReturnType<typeof setTimeout> | null = null;

  showInfo(message: string): void {
    this.errorMessage.set('');
    this.infoMessage.set(message);
    if (this.infoTimer) clearTimeout(this.infoTimer);
    this.infoTimer = setTimeout(() => this.infoMessage.set(''), 3600);
  }

  showError(message: string, error?: unknown): void {
    if (error !== undefined) console.error(`[NarraHub] ${message}`, error);
    const detail = error instanceof Error ? error.message : error == null ? '' : String(error);
    this.errorMessage.set(detail ? `${message} ${detail}` : message);
  }

  clearWorkspaceUi(): void {
    this.searchQuery.set('');
    this.focusMode.set(false);
  }

  dispose(): void {
    if (this.infoTimer) clearTimeout(this.infoTimer);
    this.infoTimer = null;
  }
}
