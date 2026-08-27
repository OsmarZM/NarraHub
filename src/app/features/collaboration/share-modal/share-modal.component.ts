import { Component, EventEmitter, Input, OnInit, Output, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { SharePermission } from '../../../core/services/online-share.service';
import { UniverseStore } from '../../library/state/universe.store';
import { CollaborationStore } from '../state/collaboration.store';

export interface ShareCreateRequest {
  universeIds: string[];
  includeChapters: boolean;
  includeEntities: boolean;
  permission: SharePermission;
  expiresInDays: number;
}

@Component({
  selector: 'app-share-modal',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './share-modal.component.html',
  styleUrl: './share-modal.component.css',
})
export class ShareModalComponent implements OnInit {
  @Input() activeUniverseId: string | null = null;
  @Output() readonly closeRequested = new EventEmitter<void>();
  @Output() readonly createRequested = new EventEmitter<ShareCreateRequest>();
  @Output() readonly info = new EventEmitter<string>();

  readonly universeStore = inject(UniverseStore);
  readonly store = inject(CollaborationStore);

  readonly selectedUniverseIds = signal<Set<string>>(new Set());
  readonly selectionCount = computed(() => this.selectedUniverseIds().size);

  includeChapters = true;
  includeEntities = true;
  permission: SharePermission = 'view';
  expiresInDays = 7;

  ngOnInit(): void {
    if (this.activeUniverseId) this.selectedUniverseIds.set(new Set([this.activeUniverseId]));
  }

  isSelected(id: string): boolean {
    return this.selectedUniverseIds().has(id);
  }

  toggleUniverse(id: string): void {
    this.selectedUniverseIds.update((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  }

  selectAll(): void {
    const universes = this.universeStore.universes();
    this.selectedUniverseIds.set(this.selectedUniverseIds().size === universes.length
      ? new Set()
      : new Set(universes.map((universe) => universe.id)));
  }

  requestCreate(): void {
    const universeIds = [...this.selectedUniverseIds()];
    if (!universeIds.length) return;
    this.createRequested.emit({
      universeIds,
      includeChapters: this.includeChapters,
      includeEntities: this.includeEntities,
      permission: this.permission,
      expiresInDays: Number(this.expiresInDays),
    });
  }

  async copyShareLink(): Promise<void> {
    const link = this.store.shareLink();
    if (!link) return;
    try {
      await navigator.clipboard.writeText(link);
      this.info.emit('Link copiado.');
    } catch (error) {
      console.warn('[NarraHub] Não foi possível copiar o link automaticamente.', error);
      this.info.emit('Não foi possível copiar automaticamente. Selecione o link manualmente.');
    }
  }

  sharePermissionLabel(permission: SharePermission): string {
    return permission === 'edit' ? 'Pode propor edições' : permission === 'comment' ? 'Somente anotações' : 'Somente leitura';
  }

  formatDate(value: string): string {
    if (!value) return 'Sem data';
    const date = new Date(value.length === 10 ? `${value}T12:00:00` : value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' });
  }
}
