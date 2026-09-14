import { DestroyRef, Injectable, inject, signal } from '@angular/core';

/**
 * Até onde a tela é "celular" para o shell.
 *
 * 760px e não um nome de aparelho: a decisão é sobre espaço, não sobre sistema. Um tablet
 * Android deitado usa o shell do desktop, e uma janela estreita do desktop usa a navegação
 * gestual — o que é justamente o jeito de exercitar o gesto sem aparelho.
 */
export const MOBILE_BREAKPOINT = '(max-width: 760px)';

@Injectable({ providedIn: 'root' })
export class ViewportState {
  readonly isMobile = signal(false);

  constructor() {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const query = window.matchMedia(MOBILE_BREAKPOINT);
    this.isMobile.set(query.matches);
    const onChange = (event: MediaQueryListEvent) => this.isMobile.set(event.matches);
    query.addEventListener('change', onChange);
    inject(DestroyRef).onDestroy(() => query.removeEventListener('change', onChange));
  }
}
