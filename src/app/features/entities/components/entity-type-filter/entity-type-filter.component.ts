import { Component, EventEmitter, Input, Output } from '@angular/core';
import { EntityHubType } from '../../state/entity.store';

interface EntityTypeTab {
  label: string;
  type: EntityHubType | null;
}

@Component({
  selector: 'app-entity-type-filter',
  standalone: true,
  templateUrl: './entity-type-filter.component.html',
  styleUrl: './entity-type-filter.component.css',
})
export class EntityTypeFilterComponent {
  @Input() selected: EntityHubType | null = null;
  @Input() counts: Record<string, number> = {};
  @Output() readonly selectedChange = new EventEmitter<EntityHubType | null>();

  readonly tabs: EntityTypeTab[] = [
    { label: 'Tudo', type: null },
    { label: 'Personagens', type: 'Personagem' },
    { label: 'Lugares', type: 'Lugar' },
    { label: 'Eventos', type: 'Evento' },
    { label: 'Objetos', type: 'Objeto' },
    { label: 'Organizações', type: 'Organização' },
  ];

  count(type: EntityHubType | null): number {
    return this.counts[type ?? 'all'] ?? 0;
  }
}
