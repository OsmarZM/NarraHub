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
    const saved = localStorage.getItem('narrahub.theme');
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
    localStorage.setItem('narrahub.theme', theme);
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
      // 1. Define no <html>
      document.documentElement.setAttribute('data-theme', resolved);
      document.documentElement.dataset['theme'] = resolved;
      document.documentElement.style.colorScheme = resolved;

      // 2. Define no <body>
      if (document.body) {
        document.body.setAttribute('data-theme', resolved);
        if (resolved === 'dark') {
          document.documentElement.classList.add('dark');
          document.documentElement.classList.remove('light');
          document.body.classList.add('dark');
          document.body.classList.remove('light');
        } else {
          document.documentElement.classList.add('light');
          document.documentElement.classList.remove('dark');
          document.body.classList.add('light');
          document.body.classList.remove('dark');
        }
      }
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
}
