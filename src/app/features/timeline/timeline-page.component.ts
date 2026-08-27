import { Component, Input, OnChanges, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ContentTag, Entity } from '../../core/models';
import { AppState } from '../../core/state/app.state';
import { EntityStore } from '../entities/state/entity.store';
import { KnowledgeStore } from '../knowledge/state/knowledge.store';
import { TimelineStore } from './state/timeline.store';

@Component({
  selector: 'app-timeline-page',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './timeline-page.component.html',
  styleUrl: './timeline-page.component.css',
})
export class TimelinePageComponent implements OnChanges {
  @Input({ required: true }) universeId = '';

  readonly store = inject(TimelineStore);
  private readonly appState = inject(AppState);
  private readonly entityStore = inject(EntityStore);
  private readonly knowledgeStore = inject(KnowledgeStore);
  readonly modal = signal<'create' | 'rename' | null>(null);
  readonly searchQuery = signal('');

  newTitle = '';
  newDate = '';
  newDescription = '';
  newEntityId = '';
  newDisplayDate = '';
  newSortKey = 0;
  renameEventId = '';
  renameTitle = '';

  ngOnChanges(): void {
    void this.store.load(this.universeId);
  }

  filteredEvents() {
    const query = this.searchQuery().trim().toLowerCase();
    const events = this.store.events();
    if (!query) return events;
    return events.filter((e) => {
      const matchTitle = (e.title || '').toLowerCase().includes(query);
      const matchDesc = (e.description || '').toLowerCase().includes(query);
      const matchDate = (e.display_date || '').toLowerCase().includes(query);
      const tags = this.tags(e.id);
      const matchTag = tags.some((t) => (t.name || '').toLowerCase().includes(query));
      return matchTitle || matchDesc || matchDate || matchTag;
    });
  }

  onTimelineWheel(event: WheelEvent): void {
    if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    const board = event.currentTarget as HTMLElement;
    if (board.scrollWidth <= board.clientWidth) return;
    event.preventDefault();
    board.scrollLeft += event.deltaY;
  }

  eventEntities(): Entity[] {
    return this.entityStore.entities().filter((entity) => entity.type === 'Evento');
  }

  tags(eventId: string): ContentTag[] {
    return this.knowledgeStore.workspacePreviewTags()[`timeline:${eventId}`] ?? [];
  }

  openCreate(): void {
    this.store.clearError();
    this.newTitle = '';
    this.newDate = '';
    this.newDescription = '';
    this.newEntityId = '';
    this.newDisplayDate = '';
    this.newSortKey = 0;
    this.modal.set('create');
  }

  openRename(eventId: string, title: string, sourceEvent: Event): void {
    sourceEvent.stopPropagation();
    this.store.clearError();
    this.renameEventId = eventId;
    this.renameTitle = title;
    this.modal.set('rename');
  }

  async create(): Promise<void> {
    const title = this.newTitle.trim();
    const displayDate = this.newDisplayDate.trim();
    if (!title || (!this.newDate && !displayDate)) return;
    const saved = await this.store.create(this.universeId, {
      title,
      date: this.newDate || '0000-01-01',
      description: this.newDescription.trim(),
      entityId: this.newEntityId || null,
      displayDate,
      sortKey: Number(this.newSortKey) || 0,
    });
    if (saved) this.modal.set(null);
  }

  async rename(): Promise<void> {
    const title = this.renameTitle.trim();
    if (!this.renameEventId || !title) return;
    if (await this.store.rename(this.universeId, this.renameEventId, title)) this.modal.set(null);
  }

  async delete(eventId: string, sourceEvent: Event): Promise<void> {
    sourceEvent.stopPropagation();
    await this.store.delete(this.universeId, eventId);
  }

  requestMetadata(eventId: string, title: string, sourceEvent: Event): void {
    sourceEvent.stopPropagation();
    void this.knowledgeStore.openMetadata('timeline', eventId, title, this.appState.activeUniverseId());
    this.appState.openModal('metadata');
  }

  closeModal(): void {
    if (!this.store.busy()) this.modal.set(null);
  }

  formatDate(value: string): string {
    if (!value) return 'Sem data';
    const date = new Date(value.length === 10 ? `${value}T12:00:00` : value);
    return Number.isNaN(date.getTime())
      ? value
      : date.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' });
  }
}

