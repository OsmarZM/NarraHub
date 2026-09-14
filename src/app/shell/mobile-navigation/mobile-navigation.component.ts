import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  NgZone,
  OnDestroy,
  afterNextRender,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
  viewChildren,
} from '@angular/core';
import {
  CARD_SPACING,
  MobileNavigationOption,
  TAP_SLOP,
  cardFrame,
  clamp,
  easeOutCubic,
  layerOpacity,
  openProgressFromDrag,
  settleIndex,
  settleOpen,
} from './mobile-navigation.model';

type Gesture =
  | { kind: 'idle' }
  | { kind: 'pulling'; startX: number; pointerId: number }
  | { kind: 'undecided'; startX: number; startY: number; startPosition: number; pointerId: number; cardIndex: number }
  | { kind: 'closing'; startX: number; pointerId: number }
  | { kind: 'wheeling'; startY: number; startPosition: number; pointerId: number };

interface Sample {
  x: number;
  y: number;
  t: number;
}

/**
 * O navegador gestual do celular.
 *
 * Recebe as opções e emite a escolha. Não conhece Router, universo, capítulo ou store — quem
 * decide o que cada id significa é o shell que o hospeda.
 *
 * ## Por que o gesto roda fora do Angular
 *
 * Um `pointermove` dispara dezenas de vezes por segundo. Se cada um passasse pela detecção de
 * mudanças, o quadro seria gasto re-verificando a árvore de componentes em vez de mover
 * cartões. Os listeners ficam fora da zona; cada quadro escreve só `transform` e `opacity`, via
 * `requestAnimationFrame`; e a zona só é chamada quando algo que o template precisa saber muda
 * de verdade — abriu, fechou, escolheu.
 *
 * ## O contrato com o shell é um atributo
 *
 * O shell marca com `data-nh-mnav-recede` o elemento que deve recuar enquanto o navegador abre,
 * e o componente escreve `transform` nele. Nada além disso: o navegador não sabe o que o elemento
 * é.
 *
 * A primeira versão escrevia uma variável CSS no `<html>` a cada quadro, e custava 3,3 ms por
 * quadro medidos num CPU de desktop. Propriedade customizada herda: mudá-la na raiz invalida o
 * estilo do documento inteiro. `transform` num elemento só não invalida estilo de ninguém.
 */
@Component({
  selector: 'app-mobile-navigation',
  standalone: true,
  templateUrl: './mobile-navigation.component.html',
  styleUrl: './mobile-navigation.component.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class MobileNavigationComponent implements OnDestroy {
  readonly options = input.required<MobileNavigationOption[]>();
  readonly activeId = input<string>('');
  readonly contextLabel = input<string>('');

  readonly selected = output<string>();

  /** Só o que o template precisa: aria, foco, e se a camada recebe toque. */
  readonly isOpen = signal(false);

  private readonly zone = inject(NgZone);
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly handle = viewChild.required<ElementRef<HTMLButtonElement>>('handle');
  private readonly layer = viewChild.required<ElementRef<HTMLElement>>('layer');
  private readonly caption = viewChild.required<ElementRef<HTMLElement>>('caption');
  private readonly cards = viewChildren<ElementRef<HTMLButtonElement>>('card');

  /** 0 fechado, 1 aberto. Muda a cada quadro durante o gesto. */
  private open = 0;
  /** Posição da roda, em índices. Fracionária durante o gesto. */
  private position = 0;
  private chosen = -1;
  private chosenPulse = 0;
  /** Último estado escrito, para não tocar `inert` e `tabIndex` quando nada mudou. */
  private readonly inertes: boolean[] = [];
  private recuado = false;
  private ativo = false;
  /** Os elementos que recuam, lidos uma vez por abertura e não a cada quadro. */
  private alvosDoRecuo: HTMLElement[] = [];
  /**
   * Largura da tela, lida fora do desenho.
   *
   * Ler `window.innerWidth` dentro do `render`, depois de escrever estilo, obriga o navegador a
   * recalcular o layout na hora — medido: o render custava 4,3 ms, dos quais escrever os oito
   * `transform` era 0,011 ms. A largura é lida no começo de cada gesto e no `resize`.
   */
  private largura = 0;

  private gesture: Gesture = { kind: 'idle' };
  private samples: Sample[] = [];
  private frame = 0;
  private animation = 0;
  private reducedMotion = false;
  private readonly cleanups: Array<() => void> = [];

  constructor() {
    // Com o navegador fechado, a roda acompanha a tela atual: ao abrir, o cartão da frente é
    // onde a pessoa está.
    effect(() => {
      const active = this.options().findIndex((option) => option.id === this.activeId());
      if (!this.isOpen() && this.gesture.kind === 'idle' && active >= 0) {
        this.position = active;
        this.requestRender();
      }
    });

    afterNextRender(() => {
      const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
      this.reducedMotion = motion.matches;
      const onMotion = (event: MediaQueryListEvent) => {
        this.reducedMotion = event.matches;
        this.requestRender();
      };
      motion.addEventListener('change', onMotion);
      this.cleanups.push(() => motion.removeEventListener('change', onMotion));

      this.largura = window.innerWidth;
      const onResize = () => {
        this.largura = window.innerWidth;
      };
      window.addEventListener('resize', onResize, { passive: true });
      this.cleanups.push(() => window.removeEventListener('resize', onResize));

      this.zone.runOutsideAngular(() => this.bindPointer());
      this.requestRender();
    });
  }

  ngOnDestroy(): void {
    cancelAnimationFrame(this.frame);
    cancelAnimationFrame(this.animation);
    this.cleanups.forEach((cleanup) => cleanup());
    document.documentElement.classList.remove('nh-mnav-active');
    document.querySelectorAll<HTMLElement>('[data-nh-mnav-recede]').forEach((alvo) => {
      alvo.style.transform = '';
      alvo.style.willChange = '';
    });
  }

  // ── caminho acessível: teclado e leitor de tela ───────────────────────────

  /**
   * O botão da alça também abre sem gesto nenhum.
   *
   * Toque gera `click` depois do `pointerup`, e esse clique brigaria com o arrasto. `detail === 0`
   * identifica ativação por teclado ou tecnologia assistiva — que é justamente o caminho que
   * não pode depender de gesto.
   */
  onHandleClick(event: MouseEvent): void {
    if (event.detail !== 0) return;
    this.zone.runOutsideAngular(() => this.animateOpen(this.open > 0.5 ? 0 : 1, () => this.focusFront()));
  }

  onCardClick(event: MouseEvent, index: number): void {
    if (event.detail !== 0) return;
    this.zone.runOutsideAngular(() => this.tapCard(index));
  }

  onLayerKeydown(event: KeyboardEvent): void {
    if (!this.isOpen()) return;
    const count = this.options().length;
    const front = Math.round(this.position);
    if (event.key === 'Escape') {
      event.preventDefault();
      this.zone.runOutsideAngular(() => this.animateOpen(0, () => this.handle().nativeElement.focus()));
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const next = clamp(front + (event.key === 'ArrowDown' ? 1 : -1), 0, count - 1);
      this.zone.runOutsideAngular(() => this.animatePosition(next, () => this.focusFront()));
    }
  }

  // ── gesto ─────────────────────────────────────────────────────────────────

  private bindPointer(): void {
    const handle = this.handle().nativeElement;
    const layer = this.layer().nativeElement;

    const listen = <K extends keyof HTMLElementEventMap>(
      target: HTMLElement,
      type: K,
      listener: (event: HTMLElementEventMap[K]) => void,
    ) => {
      target.addEventListener(type, listener as EventListener, { passive: false });
      this.cleanups.push(() => target.removeEventListener(type, listener as EventListener));
    };
    const listenWindow = (type: 'pointermove' | 'pointerup' | 'pointercancel', listener: (event: PointerEvent) => void) => {
      window.addEventListener(type, listener, { passive: false });
      this.cleanups.push(() => window.removeEventListener(type, listener));
    };

    listen(handle, 'pointerdown', (event) => {
      if (this.open > 0.5) return;
      event.preventDefault();
      cancelAnimationFrame(this.animation);
      capturar(handle, event.pointerId);
      this.gesture = { kind: 'pulling', startX: event.clientX, pointerId: event.pointerId };
      this.samples = [{ x: event.clientX, y: event.clientY, t: event.timeStamp }];
      this.host.nativeElement.classList.add('nh-mnav-pulling');
      this.requestRender();
    });

    listen(layer, 'pointerdown', (event) => {
      if (this.open < 0.99) return;
      cancelAnimationFrame(this.animation);
      // O cartão tocado é lido ANTES da captura: depois dela, todo evento do ponteiro chega
      // com a camada como alvo, e o `pointerup` não saberia mais em qual cartão o dedo estava.
      const card = (event.target as HTMLElement | null)?.closest<HTMLElement>('[data-index]');
      capturar(layer, event.pointerId);
      this.gesture = {
        kind: 'undecided',
        startX: event.clientX,
        startY: event.clientY,
        startPosition: this.position,
        pointerId: event.pointerId,
        cardIndex: card ? Number(card.dataset['index']) : -1,
      };
      this.samples = [{ x: event.clientX, y: event.clientY, t: event.timeStamp }];
    });

    const move = (event: PointerEvent) => {
      const gesture = this.gesture;
      if (gesture.kind === 'idle' || event.pointerId !== gesture.pointerId) return;
      this.record(event);

      if (gesture.kind === 'pulling') {
        this.open = openProgressFromDrag(gesture.startX - event.clientX, this.largura);
      } else if (gesture.kind === 'undecided') {
        const dx = event.clientX - gesture.startX;
        const dy = event.clientY - gesture.startY;
        if (Math.hypot(dx, dy) < TAP_SLOP) return;
        // O eixo é decidido uma vez, no primeiro movimento real, e não muda mais: um arrasto
        // que começa vertical e deriva para o lado continua girando a roda.
        this.gesture = Math.abs(dx) > Math.abs(dy) && dx > 0
          ? { kind: 'closing', startX: gesture.startX, pointerId: gesture.pointerId }
          : { kind: 'wheeling', startY: gesture.startY, startPosition: gesture.startPosition, pointerId: gesture.pointerId };
      } else if (gesture.kind === 'closing') {
        this.open = 1 - openProgressFromDrag(event.clientX - gesture.startX, this.largura);
      } else if (gesture.kind === 'wheeling') {
        const raw = gesture.startPosition - (event.clientY - gesture.startY) / CARD_SPACING;
        this.position = this.rubberBand(raw);
      }
      this.requestRender();
    };
    // Movimento e soltura na `window`, filtrados pelo `pointerId` do gesto. A primeira versão
    // ouvia só na alça e confiava na captura do ponteiro — e a captura lança exceção quando o
    // ponteiro não está ativo, o que abortava o listener e deixava o gesto parado. Sem captura,
    // o dedo sai dos 32px da alça no primeiro centímetro de arrasto. A `window` recebe os dois
    // casos; a captura continua sendo tentada, como otimização.
    listenWindow('pointermove', move);

    const end = (event: PointerEvent) => {
      const gesture = this.gesture;
      if (gesture.kind === 'idle' || event.pointerId !== gesture.pointerId) return;
      this.record(event);
      this.gesture = { kind: 'idle' };
      this.host.nativeElement.classList.remove('nh-mnav-pulling');
      const velocity = this.velocity();

      if (gesture.kind === 'pulling') {
        this.animateOpen(settleOpen(this.open, -velocity.x) ? 1 : 0);
      } else if (gesture.kind === 'closing') {
        this.animateOpen(settleOpen(this.open, -velocity.x) ? 1 : 0);
      } else if (gesture.kind === 'wheeling') {
        // px/ms vira cartões por passo de projeção: um peteleco de ~2 px/ms passa dois ou três
        // cartões, como a roda de um seletor de horário.
        this.animatePosition(settleIndex(this.position, -velocity.y / CARD_SPACING * 60, this.options().length));
      } else if (gesture.kind === 'undecided') {
        // Toque no fundo fecha; toque num cartão escolhe ou traz para a frente.
        if (gesture.cardIndex >= 0) this.tapCard(gesture.cardIndex);
        else this.animateOpen(0);
      }
    };
    listenWindow('pointerup', end);
    listenWindow('pointercancel', end);
  }

  /** Além das pontas a roda resiste, em vez de parar seco. */
  private rubberBand(raw: number): number {
    const max = this.options().length - 1;
    if (raw < 0) return raw * 0.3;
    if (raw > max) return max + (raw - max) * 0.3;
    return raw;
  }

  private record(event: PointerEvent): void {
    this.samples.push({ x: event.clientX, y: event.clientY, t: event.timeStamp });
    if (this.samples.length > 6) this.samples.shift();
  }

  /** px/ms, sobre os últimos ~80 ms. Mais antigo que isso não é a intenção de agora. */
  private velocity(): { x: number; y: number } {
    const last = this.samples[this.samples.length - 1];
    const first = this.samples.find((sample) => last.t - sample.t <= 80) ?? this.samples[0];
    const dt = Math.max(1, last.t - first.t);
    return { x: (last.x - first.x) / dt, y: (last.y - first.y) / dt };
  }

  private tapCard(index: number): void {
    const option = this.options()[index];
    if (!option) return;
    if (index !== Math.round(this.position)) {
      this.animatePosition(index, () => this.focusFront());
      return;
    }
    // O escolhido vem para a frente, os outros saem, e só então a tela muda.
    this.chosen = index;
    this.tween(180, (t) => {
      this.chosenPulse = easeOutCubic(t);
    }, () => {
      this.zone.run(() => this.selected.emit(option.id));
      this.animateOpen(0, () => {
        this.chosen = -1;
        this.chosenPulse = 0;
      });
    });
  }

  // ── animação ──────────────────────────────────────────────────────────────

  private animateOpen(target: number, done?: () => void): void {
    const from = this.open;
    const duration = this.reducedMotion ? 0 : 320 * Math.abs(target - from) + 60;
    if (target > 0.5) this.setOpenState(true);
    this.tween(duration, (t) => {
      this.open = from + (target - from) * easeOutCubic(t);
    }, () => {
      this.open = target;
      if (target < 0.5) this.setOpenState(false);
      done?.();
    });
  }

  private animatePosition(target: number, done?: () => void): void {
    const from = this.position;
    const duration = this.reducedMotion ? 0 : 260 + 40 * Math.min(3, Math.abs(target - from));
    this.tween(duration, (t) => {
      this.position = from + (target - from) * easeOutCubic(t);
    }, () => {
      this.position = target;
      done?.();
    });
  }

  private tween(duration: number, step: (t: number) => void, done: () => void): void {
    cancelAnimationFrame(this.animation);
    if (duration <= 0) {
      step(1);
      this.requestRender();
      done();
      return;
    }
    const start = performance.now();
    const tick = (now: number) => {
      const t = clamp((now - start) / duration, 0, 1);
      step(t);
      this.render();
      if (t < 1) this.animation = requestAnimationFrame(tick);
      else done();
    };
    this.animation = requestAnimationFrame(tick);
  }

  private setOpenState(open: boolean): void {
    if (this.isOpen() === open) return;
    this.host.nativeElement.classList.toggle('nh-mnav-open', open);
    this.zone.run(() => this.isOpen.set(open));
  }

  private focusFront(): void {
    const front = this.cards()[Math.round(this.position)];
    if (this.isOpen() && front) front.nativeElement.focus({ preventScroll: true });
  }

  // ── desenho ───────────────────────────────────────────────────────────────

  private requestRender(): void {
    cancelAnimationFrame(this.frame);
    this.frame = requestAnimationFrame(() => this.render());
  }

  /** O único lugar que escreve no DOM por quadro. Só `transform`, `opacity` e `z-index`. */
  private render(): void {
    const open = this.open;
    const visivel = open > 0.001;

    // A classe no `<html>` muda duas vezes por abertura, não a cada quadro.
    if (visivel !== this.ativo) {
      this.ativo = visivel;
      document.documentElement.classList.toggle('nh-mnav-active', visivel);
      if (visivel) {
        this.alvosDoRecuo = Array.from(document.querySelectorAll<HTMLElement>('[data-nh-mnav-recede]'));
      }
    }

    // O conteúdo recua. Sem transformação quando fechado: um `transform` permanente viraria bloco
    // de contenção para todo `position: fixed` dentro dele, e os modais se posicionariam pelo
    // palco em vez de pela tela.
    const alvos = this.alvosDoRecuo;
    if (visivel) {
      const recuo = this.reducedMotion
        ? ''
        : `translate3d(${(-open * 9).toFixed(3)}%, 0, 0) scale(${(1 - open * 0.1).toFixed(4)})`;
      alvos.forEach((alvo) => {
        alvo.style.transform = recuo;
        if (!this.recuado) alvo.style.willChange = 'transform';
      });
      this.recuado = true;
    } else if (this.recuado) {
      alvos.forEach((alvo) => {
        alvo.style.transform = '';
        alvo.style.willChange = '';
      });
      this.recuado = false;
    }

    const layer = this.layer().nativeElement;
    layer.style.opacity = String(layerOpacity(open));
    // A legenda só aparece na reta final: no meio do gesto ela cruzava o logo do cabeçalho, que
    // ainda está visível atrás da camada.
    this.caption().nativeElement.style.opacity = String(clamp((open - 0.7) / 0.3, 0, 1));
    layer.style.visibility = open > 0.001 ? 'visible' : 'hidden';

    const handle = this.handle().nativeElement;
    const pull = this.gesture.kind === 'pulling' ? open : 0;
    handle.style.transform = `translate3d(${-pull * this.largura * 0.18}px, 0, 0)`;
    handle.style.opacity = String(clamp(1 - open * 1.6, 0, 1));

    this.cards().forEach((card, index) => {
      const pose = cardFrame(index - this.position, open, this.reducedMotion);
      const element = card.nativeElement;
      const chosen = index === this.chosen;
      const pulse = this.chosenPulse;
      element.style.transform = chosen && !this.reducedMotion
        ? `${pose.transform} scale(${1 + pulse * 0.06})`
        : pose.transform;
      element.style.opacity = String(this.chosen >= 0 && !chosen ? pose.opacity * (1 - pulse * 0.8) : pose.opacity);
      element.style.zIndex = String(pose.zIndex);
      if (this.inertes[index] !== pose.inert) {
        this.inertes[index] = pose.inert;
        element.tabIndex = pose.inert ? -1 : 0;
        element.toggleAttribute('inert', pose.inert);
      }
    });
  }
}

/**
 * Captura o ponteiro quando dá. Falhar não é erro: o gesto funciona pela `window` de qualquer
 * forma, e a captura só garante que nenhum outro elemento reaja ao mesmo dedo.
 */
function capturar(elemento: HTMLElement, pointerId: number): void {
  try {
    elemento.setPointerCapture(pointerId);
  } catch {
    // Ponteiro já solto, ou sintético. Segue sem captura.
  }
}
