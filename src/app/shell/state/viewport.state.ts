import { DestroyRef, Injectable, inject, signal } from '@angular/core';
import { MOBILE_MEDIA_QUERY } from './breakpoints';

/**
 * Qual shell a aplicação usa: o do desktop ou o mobile.
 *
 * O critério está em `breakpoints.ts` (toque + lado menor ≤ 760px). Quando é mobile, o `<html>`
 * recebe a classe `nh-mobile` — é nela que se apoiam as regras de página inteira (zoom, rolagem
 * elástica, altura dinâmica) de `src/styles/mobile.css`.
 *
 * Para exercitar no navegador: emular um aparelho com toque (o preset "mobile" do DevTools).
 */
@Injectable({ providedIn: 'root' })
export class ViewportState {
  readonly isMobile = signal(false);

  constructor() {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const query = window.matchMedia(MOBILE_MEDIA_QUERY);
    const apply = (matches: boolean) => {
      this.isMobile.set(matches);
      document.documentElement.classList.toggle('nh-mobile', matches);
    };
    apply(query.matches);
    const onChange = (event: MediaQueryListEvent) => apply(event.matches);
    query.addEventListener('change', onChange);
    inject(DestroyRef).onDestroy(() => query.removeEventListener('change', onChange));
  }
}
