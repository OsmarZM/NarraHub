import { Component, EventEmitter, Input, Output } from '@angular/core';
import { EntityHubType } from '../../state/entity.store';
import { EntityTypeFilterComponent } from '../entity-type-filter/entity-type-filter.component';

@Component({
  selector: 'app-entity-toolbar',
  standalone: true,
  imports: [EntityTypeFilterComponent],
  templateUrl: './entity-toolbar.component.html',
  styleUrl: './entity-toolbar.component.css',
})
export class EntityToolbarComponent {
  @Input() visibleCount = 0;
  @Input() createLabel = 'Nova entidade';
  @Input() selectedType: EntityHubType | null = null;
  @Input() counts: Record<string, number> = {};
  @Output() readonly createRequested = new EventEmitter<void>();
  @Output() readonly typeSelected = new EventEmitter<EntityHubType | null>();
}
