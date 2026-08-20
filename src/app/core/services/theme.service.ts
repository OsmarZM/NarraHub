import { Injectable, signal } from '@angular/core';

export type ThemePreference = 'light' | 'dark' | 'system';

@Injectable({ providedIn: 'root' })
export class ThemeService {
  readonly preference = signal<ThemePreference>('system');
  readonly resolvedTheme = signal<'light' | 'dark'>('light');
  private readonly media = window.matchMedia('(prefers-color-scheme: dark)');

  constructor() {
    const saved = localStorage.getItem('narrahub.theme');
    if (saved === 'light' || saved === 'dark' || saved === 'system') this.preference.set(saved);
    this.apply();
    this.media.addEventListener('change', () => {
      if (this.preference() === 'system') this.apply();
    });
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
    const resolved: 'light' | 'dark' = this.preference() === 'system'
      ? (this.media.matches ? 'dark' : 'light')
      : this.preference() as 'light' | 'dark';
    this.resolvedTheme.set(resolved);
    document.documentElement.dataset['theme'] = resolved;
    document.documentElement.style.colorScheme = resolved;
  }
}
