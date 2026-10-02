import { Component, inject, input, output, signal } from '@angular/core';
import { MobileSheetComponent } from '../mobile-sheet/mobile-sheet.component';
import { MobileTopbarComponent } from '../mobile-topbar/mobile-topbar.component';
import { ShellActionsState } from '../state/shell-actions.state';

/**
 * O shell do celular.
 *
 * Mesma aplicação do desktop — o mesmo Router, os mesmos stores, serviços e ações —, outra
 * composição visual:
 *
 *   ┌────────────────────────────┐
 *   │ ‹ Universo        🔍  •••  │  app-mobile-topbar
 *   ├────────────────────────────┤
 *   │                            │
 *   │         conteúdo           │  a rota (projetada pelo RootLayout)
 *   │                            ╎  alça da navegação gestual (projetada)
 *   │                            │
 *   └────────────────────────────┘
 *
 * Sem sidebar, sem barra de baixo, sem cabeçalho de desktop. As ações secundárias da tela chegam
 * pelo `ShellActionsState` e abrem numa folha. Arquitetura em docs/mobile/README.md.
 */
@Component({
  selector: 'app-mobile-shell',
  standalone: true,
  imports: [MobileTopbarComponent, MobileSheetComponent],
  template: `
    <div class="nh-mshell-sky" aria-hidden="true"><i class="nh-mshell-stars"></i></div>

    <app-mobile-topbar
      [title]="title()"
      [eyebrow]="eyebrow()"
      [canGoBack]="canGoBack()"
      [hasActions]="actions.actions().length > 0"
      [query]="query()"
      [searchLabel]="searchLabel()"
      (queryChange)="queryChange.emit($event)"
      (back)="back.emit()"
      (actionsRequested)="actionsOpen.set(true)"
    />

    <main class="nh-mshell-content" data-nh-mnav-recede>
      <ng-content />
    </main>

    <ng-content select="[navigation]" />

    @if (actionsOpen()) {
      <app-mobile-sheet [title]="actions.title() || 'Ações'" [eyebrow]="actions.status()" (closed)="actionsOpen.set(false)">
        <ul class="nh-mshell-actions">
          @for (action of actions.actions(); track action.id) {
            <li>
              <button type="button" [disabled]="action.disabled" [class.active]="action.active" [attr.aria-pressed]="action.active ?? null" (click)="run(action.run)">
                <span class="nh-mshell-action-icon" aria-hidden="true">{{ action.icon }}</span>
                <span>{{ action.label }}</span>
              </button>
            </li>
          }
        </ul>
      </app-mobile-sheet>
    }
  `,
  styleUrl: './mobile-shell.component.css',
})
export class MobileShellComponent {
  readonly title = input('NarraHub');
  readonly eyebrow = input('');
  readonly canGoBack = input(false);
  readonly query = input('');
  readonly searchLabel = input('Buscar');
  readonly queryChange = output<string>();
  readonly back = output<void>();

  readonly actions = inject(ShellActionsState);
  readonly actionsOpen = signal(false);

  run(action: () => void): void {
    this.actionsOpen.set(false);
    action();
  }
}
