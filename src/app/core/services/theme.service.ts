import { Injectable, signal } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

export type ThemePreference = 'light' | 'dark' | 'system';

@Injectable({ providedIn: 'root' })
export class ThemeService {
  readonly preference = signal<ThemePreference>('system');
  readonly resolvedTheme = signal<'light' | 'dark'>('light');
  private readonly media: MediaQueryList | null =
    typeof window !== 'undefined' && window.matchMedia ? window.matchMedia('(prefers-color-scheme: dark)') : null;

  constructor() {
    const saved = this.readSavedPreference();
    if (saved === 'light' || saved === 'dark' || saved === 'system') {
      this.preference.set(saved);
    }
    this.apply();

    if (this.media) {
      this.media.addEventListener('change', () => {
        if (this.preference() === 'system') {
          this.apply();
        }
      });
    }
  }

  setTheme(theme: ThemePreference): void {
    this.preference.set(theme);
    try { localStorage.setItem('narrahub.theme', theme); } catch { /* storage unavailable */ }
    this.apply();
  }

  toggle(): void {
    this.setTheme(this.resolvedTheme() === 'dark' ? 'light' : 'dark');
  }

  private apply(): void {
    const isSystemDark = this.media ? this.media.matches : false;
    const resolved: 'light' | 'dark' = this.preference() === 'system'
      ? (isSystemDark ? 'dark' : 'light')
      : (this.preference() as 'light' | 'dark');

    this.resolvedTheme.set(resolved);

    if (typeof document !== 'undefined') {
      this.applyToElement(document.documentElement, resolved);
      if (document.body) this.applyToElement(document.body, resolved);
    }

    // 3. Sincroniza com a janela nativa do Windows 11 via Tauri se estiver no Desktop
    if (isTauri()) {
      try {
        const win = getCurrentWindow();
        if (win && typeof win.setTheme === 'function') {
          win.setTheme(this.preference() === 'system' ? null : resolved).catch(() => {});
        }
      } catch (_) {}
    }
  }

  private applyToElement(element: HTMLElement, theme: 'light' | 'dark'): void {
    element.dataset['theme'] = theme;
    element.classList.remove('light', 'dark');
    element.classList.add(theme);
    element.style.colorScheme = theme;
  }

  private readSavedPreference(): string | null {
    try { return localStorage.getItem('narrahub.theme'); }
    catch { return null; }
  }
}
