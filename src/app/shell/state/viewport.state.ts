import { DestroyRef, Injectable, inject, signal } from '@angular/core';

/**
 * Quando o shell é o do CELULAR ANDROID.
 *
 * Duas condições: o sistema é Android e a tela é de celular. O desktop não recebe o layout do
 * celular nem com a janela estreita — a decisão do humano foi um layout só para Android. Um
 * tablet Android largo continua no shell do desktop.
 *
 * Para exercitar no navegador, basta emular um aparelho Android (o user agent traz "Android").
 */
export const MOBILE_BREAKPOINT = '(max-width: 900px)';

export function isAndroidUserAgent(userAgent: string): boolean {
  return /\bAndroid\b/iu.test(userAgent);
}

@Injectable({ providedIn: 'root' })
export class ViewportState {
  readonly isAndroid = typeof navigator !== 'undefined' && isAndroidUserAgent(navigator.userAgent);
  readonly isMobile = signal(false);

  constructor() {
    if (typeof window === 'undefined' || !window.matchMedia || !this.isAndroid) return;
    const query = window.matchMedia(MOBILE_BREAKPOINT);
    const apply = (matches: boolean) => {
      this.isMobile.set(matches);
      // No <html>: o que vale para a página inteira (rolagem elástica, seleção por toque longo)
      // não pode depender de onde o componente raiz está.
      document.documentElement.classList.toggle('nh-android', matches);
    };
    apply(query.matches);
    const onChange = (event: MediaQueryListEvent) => apply(event.matches);
    query.addEventListener('change', onChange);
    inject(DestroyRef).onDestroy(() => query.removeEventListener('change', onChange));
  }
}
