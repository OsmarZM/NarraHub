import { Component, input, output } from '@angular/core';

@Component({
  selector: 'app-contextual-inspector',
  standalone: true,
  template: `<aside class="nh-inspector" [attr.aria-label]="label()"><ng-content></ng-content></aside>`,
  styleUrl: './contextual-inspector.component.css',
})
export class ContextualInspectorComponent {
  readonly label = input('Detalhes');
  readonly closed = output<void>();
}
