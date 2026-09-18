import { Component, ElementRef, HostListener, OnInit, input, output, viewChild } from '@angular/core';

/**
 * Folha que sobe da parte de baixo da tela — o "modal" do celular.
 *
 * Usada para ações secundárias, árvore de capítulos, resumo e formulários curtos. Cobre a tela
 * com um fundo escurecido; toque no fundo, no "fechar" ou Esc fecha. A altura é % da tela, que já
 * encolhe com o teclado, e o fim respeita a safe area de baixo.
 *
 * Só aparece no shell mobile. Nada de `backdrop-filter`: é translucidez sobre cor, que o WebView
 * desenha barato.
 */
@Component({
  selector: 'app-mobile-sheet',
  standalone: true,
  template: `
    <div class="nh-sheet-backdrop" (click)="closed.emit()"></div>
    <section
      #panel
      class="nh-sheet"
      [class.tall]="size() === 'tall'"
      role="dialog"
      aria-modal="true"
      [attr.aria-label]="title() || null"
      tabindex="-1"
    >
      <div class="nh-sheet-grip" aria-hidden="true"></div>
      @if (title()) {
        <header class="nh-sheet-header">
          <div>
            @if (eyebrow()) { <small>{{ eyebrow() }}</small> }
            <h2>{{ title() }}</h2>
          </div>
          <button type="button" class="nh-sheet-close" aria-label="Fechar" (click)="closed.emit()">✕</button>
        </header>
      }
      <div class="nh-sheet-body"><ng-content /></div>
    </section>
  `,
  styleUrl: './mobile-sheet.component.css',
})
export class MobileSheetComponent implements OnInit {
  readonly title = input('');
  readonly eyebrow = input('');
  /** `auto`: do tamanho do conteúdo. `tall`: ocupa quase a tela (listas longas, formulários). */
  readonly size = input<'auto' | 'tall'>('auto');
  readonly closed = output<void>();

  private readonly panel = viewChild.required<ElementRef<HTMLElement>>('panel');

  ngOnInit(): void {
    // Foco na folha: leitor de tela anuncia o diálogo, e Esc chega aqui.
    queueMicrotask(() => this.panel().nativeElement.focus({ preventScroll: true }));
  }

  @HostListener('document:keydown.escape')
  onEscape(): void {
    this.closed.emit();
  }
}
