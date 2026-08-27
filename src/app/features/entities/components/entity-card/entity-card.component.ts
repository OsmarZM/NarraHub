import { Component, EventEmitter, Input, Output } from '@angular/core';
import { ContentTag, Entity } from '../../../../core/models';

@Component({
  selector: 'app-entity-card',
  standalone: true,
  templateUrl: './entity-card.component.html',
  styleUrl: './entity-card.component.css',
})
export class EntityCardComponent {
  @Input({ required: true }) entity!: Entity;
  @Input() tags: ContentTag[] = [];
  @Input() delay = 0;
  @Output() readonly opened = new EventEmitter<Entity>();
  @Output() readonly renameRequested = new EventEmitter<{ entity: Entity; sourceEvent: Event }>();
  @Output() readonly deleteRequested = new EventEmitter<{ entity: Entity; sourceEvent: Event }>();
}
