/**
 * A escala oficial de breakpoints do NarraHub.
 *
 *   mobile   lado MENOR da tela ≤ 760px, com toque (pointer: coarse)
 *   tablet   largura ≤ 1024px
 *   desktop  largura > 1024px
 *
 * CSS não aceita variável dentro de `@media`, então os números vivem aqui e em
 * `docs/mobile/README.md`; todo CSS novo usa estes valores. O legado (520, 680, 700, 900, 1450…)
 * não foi migrado de uma vez — ver `docs/mobile/AUDITORIA.md` §9.
 */
export const BREAKPOINTS = {
  mobileShortSide: 760,
  tablet: 1024,
} as const;

/**
 * Quando o shell é o MOBILE.
 *
 * Toque + lado menor ≤ 760px. O lado menor, e não a largura: um celular deitado tem ~850px de
 * largura e continua sendo um celular. O toque, e não o user agent: uma janela estreita de
 * desktop, com mouse, continua no shell do desktop; um tablet grande também.
 */
export const MOBILE_MEDIA_QUERY =
  `(pointer: coarse) and (max-width: ${BREAKPOINTS.mobileShortSide}px), ` +
  `(pointer: coarse) and (max-height: ${BREAKPOINTS.mobileShortSide}px)`;

export interface ViewportSample {
  width: number;
  height: number;
  coarsePointer: boolean;
}

/** A mesma decisão da media query, como função pura — é o que os testes exercitam. */
export function isMobileViewport({ width, height, coarsePointer }: ViewportSample): boolean {
  return coarsePointer && Math.min(width, height) <= BREAKPOINTS.mobileShortSide;
}
