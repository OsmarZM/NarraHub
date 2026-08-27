import { CommonModule } from '@angular/common';
import { Component, EventEmitter, Input, Output, inject } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ContentTag } from '../../../core/models';
import { KnowledgeStore } from '../state/knowledge.store';

@Component({
  selector: 'app-tags-modal',
  standalone: true,
  imports: [CommonModule, FormsModule],
  templateUrl: './tags-modal.component.html',
  styleUrl: './tags-modal.component.css',
})
export class TagsModalComponent {
  @Input() activeUniverseId: string | null = null;
  @Output() readonly closeRequested = new EventEmitter<void>();
  @Output() readonly failed = new EventEmitter<string>();

  readonly store = inject(KnowledgeStore);

  newTagName = '';
  newTagColor = '#7d3650';

  close(): void {
    this.store.closeMetadata();
    this.closeRequested.emit();
  }

  async createTag(): Promise<void> {
    const name = this.newTagName.trim();
    if (!name) return;
    if (!await this.store.createTag(name, this.newTagColor, this.activeUniverseId)) {
      this.failed.emit(this.store.error() || 'Não foi possível criar a tag. Verifique se esse nome já existe.');
      return;
    }
    this.newTagName = '';
  }

  async toggleTag(tag: ContentTag): Promise<void> {
    if (!await this.store.toggleTag(tag, this.activeUniverseId)) this.failed.emit(this.store.error() || 'Não foi possível atualizar a tag.');
  }

  async deleteTag(tag: ContentTag): Promise<void> {
    if (!window.confirm(`Excluir a tag “${tag.name}” de todo o universo?`)) return;
    if (!await this.store.deleteTag(tag, this.activeUniverseId)) this.failed.emit(this.store.error() || 'Não foi possível excluir a tag.');
  }
}
