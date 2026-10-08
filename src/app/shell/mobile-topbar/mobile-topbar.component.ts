import { Component, ElementRef, input, output, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';

/**
 * A barra de cima do shell mobile.
 *
 *   biblioteca      NARRAHUB                 🔍  •••
 *   universo        ‹  História               🔍  •••
 *                      Hopi horror
 *
 * Três alvos de 44px no máximo, e nenhum texto de ação: o que não é voltar, buscar ou "•••" vai
 * para a folha de ações. A busca abre ocupando a barra inteira. Navegar entre áreas é a alça
 * gestual; esta barra não tem menu de navegação.
 */
@Component({
  selector: 'app-mobile-topbar',
  standalone: true,
  imports: [FormsModule],
  template: `
    <header class="nh-mtop" [class.searching]="searching()">
      @if (searching()) {
        <label class="nh-mtop-search">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="m16.4 16.4 4.1 4.1"></path></svg>
          <input
            #searchInput
            type="search"
            enterkeyhint="search"
            [attr.aria-label]="searchLabel()"
            [placeholder]="searchLabel()"
            [ngModel]="query()"
            (ngModelChange)="queryChange.emit($event)"
          />
        </label>
        <button type="button" class="nh-mtop-icon" aria-label="Fechar busca" (click)="closeSearch()">✕</button>
      } @else {
        @if (canGoBack()) {
          <button type="button" class="nh-mtop-icon nh-mtop-back" aria-label="Voltar aos universos" (click)="back.emit()">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M15 5 8 12l7 7"></path></svg>
          </button>
        }
        <div class="nh-mtop-title" [class.with-back]="canGoBack()">
          @if (eyebrow()) { <small>{{ eyebrow() }}</small> }
          <strong [class.wordmark]="title() === 'NarraHub'">{{ title() }}</strong>
        </div>
        <button type="button" class="nh-mtop-icon" [attr.aria-label]="searchLabel()" (click)="openSearch()">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="m16.4 16.4 4.1 4.1"></path></svg>
        </button>
        @if (hasActions()) {
          <button type="button" class="nh-mtop-icon" aria-label="Mais ações" aria-haspopup="dialog" (click)="actionsRequested.emit()">
            <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="5" cy="12" r="1.6"></circle><circle cx="12" cy="12" r="1.6"></circle><circle cx="19" cy="12" r="1.6"></circle></svg>
          </button>
        }
      }
    </header>
  `,
  styleUrl: './mobile-topbar.component.css',
})
export class MobileTopbarComponent {
  readonly title = input('NarraHub');
  readonly eyebrow = input('');
  readonly canGoBack = input(false);
  readonly hasActions = input(false);
  readonly query = input('');
  readonly searchLabel = input('Buscar');
  readonly queryChange = output<string>();
  readonly back = output<void>();
  readonly actionsRequested = output<void>();

  readonly searching = signal(false);
  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');

  openSearch(): void {
    this.searching.set(true);
    setTimeout(() => this.searchInput()?.nativeElement.focus());
  }

  closeSearch(): void {
    this.queryChange.emit('');
    this.searching.set(false);
  }
}
