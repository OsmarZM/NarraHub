import { Component, ViewEncapsulation, input } from '@angular/core';

@Component({
  selector: 'app-shell',
  standalone: true,
  templateUrl: './app-shell.component.html',
  styleUrl: './app-shell.component.css',
  encapsulation: ViewEncapsulation.None,
})
export class AppShellComponent {
  readonly showSidebar = input(false);
  readonly focusMode = input(false);
}
