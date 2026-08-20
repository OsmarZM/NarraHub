import { Component, input, output } from '@angular/core';
import { UniverseWithStats } from '../../core/models';

export interface SidebarNavItem {
  id: string;
  label: string;
  icon: string;
  needsUniverse: boolean;
}

@Component({
  selector: 'app-universe-sidebar',
  standalone: true,
  templateUrl: './universe-sidebar.component.html',
  styleUrl: './universe-sidebar.component.css',
})
export class UniverseSidebarComponent {
  readonly universe = input.required<UniverseWithStats>();
  readonly items = input.required<SidebarNavItem[]>();
  readonly activeItem = input.required<string>();
  readonly collapsed = input(false);
  readonly itemSelected = output<SidebarNavItem>();
  readonly changeUniverse = output<void>();
  readonly collapseToggled = output<void>();
}
