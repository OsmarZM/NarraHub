import { Component, ElementRef, input, output, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';

/**
 * A barra de cima do Android: título da tela e busca.
 *
 * Substitui a barra de título do desktop, que no celular era logo + busca espremidos por baixo
 * da barra de status, com controles de janela que não existem num telefone. Aqui a busca fica
 * atrás de um botão e ocupa a barra inteira quando aberta, como nos apps nativos. Navegar é a
 * alça gestual; esta barra não tem menu.
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
            [attr.aria-label]="workspaceMode() ? 'Buscar no universo' : 'Buscar universos'"
            [placeholder]="workspaceMode() ? 'Buscar no universo' : 'Buscar universos'"
            [ngModel]="query()"
            (ngModelChange)="queryChange.emit($event)"
          />
        </label>
        <button type="button" class="nh-mtop-icon" aria-label="Fechar busca" (click)="closeSearch()">✕</button>
      } @else {
        <button type="button" class="nh-mtop-brand" aria-label="Voltar à biblioteca de universos" (click)="homeRequested.emit()">
          <img src="assets/narrahub-logo-full.webp" alt="" />
        </button>
        <div class="nh-mtop-title">
          @if (context()) { <small>{{ context() }}</small> }
          <strong>{{ title() }}</strong>
        </div>
        <button type="button" class="nh-mtop-icon" aria-label="Buscar" (click)="openSearch()">
          <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="m16.4 16.4 4.1 4.1"></path></svg>
        </button>
      }
    </header>
  `,
  styleUrl: './mobile-topbar.component.css',
})
export class MobileTopbarComponent {
  readonly title = input('NarraHub');
  readonly context = input('');
  readonly query = input('');
  readonly workspaceMode = input(false);
  readonly queryChange = output<string>();
  readonly homeRequested = output<void>();

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
