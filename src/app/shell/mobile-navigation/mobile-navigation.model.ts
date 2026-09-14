/**
 * A navegação gestual do NarraHub no celular — modelo e física.
 *
 * Tudo aqui é função pura: nada toca DOM, Router ou store. O componente lê o dedo, pede a
 * estas funções o que desenhar, e escreve só `transform` e `opacity`. É o que mantém o gesto
 * a 60 fps num Android modesto — nenhuma propriedade que dispare layout muda por quadro — e o
 * que permite testar a física sem navegador.
 *
 * O gesto tem duas dimensões, e a separação é decisão de UX:
 *
 *   horizontal   puxar a alça da borda direita ABRE; empurrar para a direita FECHA
 *   vertical     com o navegador aberto, girar os cartões como a roda de um seletor
 *
 * Com os dois no mesmo eixo, cada arrasto ambíguo seria uma aposta.
 */

/** Um destino, como o navegador precisa ver. Não sabe como a tela guarda dados. */
export interface MobileNavigationOption {
  id: string;
  label: string;
  icon: string;
  description: string;
  /** Precisa de um universo aberto e nenhum está. O cartão aparece, com a dica. */
  needsContext: boolean;
}

/** Quanto da largura da tela o dedo precisa percorrer para o navegador ficar 100% aberto. */
export const OPEN_TRAVEL_RATIO = 0.62;

/** Soltou depois deste ponto: abre. Antes: volta. */
export const OPEN_SNAP_THRESHOLD = 0.34;

/** Um arrasto rápido abre ou fecha mesmo sem chegar ao limiar. px/ms. */
export const FLING_VELOCITY = 0.55;

/** Distância vertical entre um cartão e o próximo, em px. */
export const CARD_SPACING = 108;

/** Movimento abaixo disto é toque, não arrasto. px. */
export const TAP_SLOP = 8;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Quanto o navegador está aberto, de 0 a 1, dado o arrasto horizontal desde a borda. */
export function openProgressFromDrag(dragLeftPx: number, viewportWidth: number): number {
  return clamp(dragLeftPx / Math.max(1, viewportWidth * OPEN_TRAVEL_RATIO), 0, 1);
}

/**
 * Onde o navegador para quando o dedo solta.
 *
 * A velocidade manda antes da posição: um puxão curto e rápido é intenção clara de abrir,
 * e obrigar a pessoa a arrastar até a metade da tela transformaria o gesto num esforço.
 * `velocityLeft` é positivo quando o dedo ia para a esquerda (abrindo).
 */
export function settleOpen(progress: number, velocityLeft: number): boolean {
  if (velocityLeft > FLING_VELOCITY) return true;
  if (velocityLeft < -FLING_VELOCITY) return false;
  return progress >= OPEN_SNAP_THRESHOLD;
}

/**
 * Para qual cartão a roda gira quando o dedo solta.
 *
 * Um peteleco projeta a posição à frente, como a roda de um relógio digital: quanto mais rápido,
 * mais cartões passam. A projeção é limitada a três para um gesto nervoso não atravessar a
 * lista inteira.
 */
export function settleIndex(position: number, velocityUp: number, count: number): number {
  const projected = position + clamp(velocityUp * 2.2, -3, 3);
  return clamp(Math.round(projected), 0, Math.max(0, count - 1));
}

/** O que o componente escreve num cartão. Só propriedades de composição. */
export interface CardFrame {
  transform: string;
  opacity: number;
  zIndex: number;
  /** Cartão longe demais para ser tocado. */
  inert: boolean;
}

/**
 * A pose de um cartão.
 *
 * @param offset   índice do cartão menos a posição da roda. 0 é o cartão da frente.
 * @param open     quanto o navegador está aberto, 0..1.
 * @param reduced  `prefers-reduced-motion`: sem perspectiva, sem escala, só presença.
 *
 * Cada cartão entra com um pequeno atraso proporcional à distância do centro — o da frente
 * chega primeiro, os de trás vêm depois. É isso que dá a sensação de pilha em profundidade em
 * vez de uma lista deslizando em bloco.
 */
export function cardFrame(offset: number, open: number, reduced: boolean): CardFrame {
  const distance = Math.abs(offset);
  const entrance = clamp(open * 1.35 - distance * 0.12, 0, 1);

  if (reduced) {
    const visible = distance < 2.5;
    return {
      transform: `translate3d(0, ${offset * CARD_SPACING}px, 0)`,
      opacity: visible ? entrance * (distance < 0.5 ? 1 : 0.6) : 0,
      zIndex: 100 - Math.round(distance * 10),
      inert: !visible || open < 0.99,
    };
  }

  const y = offset * CARD_SPACING * (1 - distance * 0.06);
  const scale = 1 - Math.min(distance, 3) * 0.1;
  const tilt = clamp(offset * -16, -48, 48);
  const depth = -Math.min(distance, 3) * 70;
  // Entram desde a borda da alça, e não já no centro: com o conteúdo ainda visível atrás, um
  // cartão surgindo no meio da tela produzia dupla exposição — o título da página atravessando
  // o cartão. Vista na captura a 45% de abertura, e corrigida aqui.
  const slideIn = (1 - entrance) * 95;
  const fade = distance > 2.4 ? 0 : 1 - distance * 0.34;

  return {
    transform:
      `translate3d(${slideIn}%, ${y}px, ${depth}px) rotateX(${tilt}deg) scale(${scale})`,
    opacity: clamp(fade * entrance, 0, 1),
    zIndex: 100 - Math.round(distance * 10),
    inert: distance > 2.4 || open < 0.99,
  };
}

/**
 * Curva de saída suave: rápida no começo, assentando no fim. Aproxima uma mola criticamente
 * amortecida sem simulação — suficiente para um snap que não quica.
 */
export function easeOutCubic(t: number): number {
  const u = 1 - clamp(t, 0, 1);
  return 1 - u * u * u;
}

/**
 * Opacidade da camada de fundo para uma abertura.
 *
 * Mais rápida que o dedo: a camada fica sólida em ~60% do arrasto, então o conteúdo some antes de
 * os cartões chegarem ao centro, e os dois nunca aparecem sobrepostos em intensidade parecida.
 */
export function layerOpacity(open: number): number {
  return clamp(open * 1.7, 0, 1);
}
