import { Component, EventEmitter, Input, Output } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Attachment, ContentTag, EntityAttribute, EntityWithDetails } from '../../../core/models';

@Component({
  selector: 'app-entity-sheet',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './entity-sheet.component.html',
  styleUrl: './entity-sheet.component.css',
})
export class EntitySheetComponent {
  @Input({ required: true }) entity!: EntityWithDetails;
  @Input() gallery: Attachment[] = [];
  @Input() tags: ContentTag[] = [];
  @Input() aiBusy = false;
  @Input() aiError = '';
  @Input() busy = false;
  @Output() readonly backRequested = new EventEmitter<void>();
  @Output() readonly metadataRequested = new EventEmitter<void>();
  @Output() readonly deleteRequested = new EventEmitter<void>();
  @Output() readonly saveRequested = new EventEmitter<void>();
  @Output() readonly patchRequested = new EventEmitter<{ field: 'name' | 'description' | 'summary' | 'canon_status'; value: string }>();
  @Output() readonly imageSelected = new EventEmitter<Event>();
  @Output() readonly imageRemoveRequested = new EventEmitter<void>();
  @Output() readonly summaryRequested = new EventEmitter<void>();
  @Output() readonly fieldsSuggested = new EventEmitter<void>();
  @Output() readonly attributeAdded = new EventEmitter<void>();
  @Output() readonly attributeRemoved = new EventEmitter<EntityAttribute>();
  @Output() readonly gallerySelected = new EventEmitter<Event>();
  @Output() readonly galleryImageDeleted = new EventEmitter<string>();

  extractFirstUrl(text?: string): string | null {
    if (!text) return null;
    return text.match(/https?:\/\/[^\s]+/iu)?.[0] ?? null;
  }

  openExternalLink(url: string): void {
    if (url) window.open(url, '_blank', 'noopener,noreferrer');
  }
}
