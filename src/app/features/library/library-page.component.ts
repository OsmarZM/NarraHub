import { Component, EventEmitter, Input, OnInit, Output, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { isTauri } from '@tauri-apps/api/core';
import { ContentTag, UniverseWithStats } from '../../core/models';
import { fileToDataUrl } from '../../shared/utils/file-to-data-url';
import { UniversePickerComponent } from '../universe-picker/universe-picker.component';
import { UniverseStore } from './state/universe.store';

type LibraryModal = 'create' | 'edit' | 'delete' | null;

@Component({
  selector: 'app-library-page',
  standalone: true,
  imports: [FormsModule, UniversePickerComponent],
  templateUrl: './library-page.component.html',
  styleUrl: './library-page.component.css',
})
export class LibraryPageComponent implements OnInit {
  @Input() query = '';
  @Input() lastOpenedId: string | null = null;
  @Input() tagsByUniverse: Record<string, ContentTag[]> = {};
  @Output() readonly opened = new EventEmitter<UniverseWithStats>();
  @Output() readonly updated = new EventEmitter<void>();
  @Output() readonly deleted = new EventEmitter<string>();

  readonly store = inject(UniverseStore);
  readonly modal = signal<LibraryModal>(null);
  readonly pendingDelete = signal<UniverseWithStats | null>(null);

  editingUniverseId: string | null = null;
  nameInput = '';
  descriptionInput = '';
  coverImageInput = '';
  deleteConfirmation = '';

  ngOnInit(): void {
    // Fora do runtime Tauri (ex.: `ng serve` para iteração de UI) não há banco local disponível.
    if (isTauri()) void this.store.load();
  }

  openCreate(): void {
    this.store.clearError();
    this.editingUniverseId = null;
    this.nameInput = '';
    this.descriptionInput = '';
    this.coverImageInput = '';
    this.modal.set('create');
  }

  openEdit(universe: UniverseWithStats): void {
    this.store.clearError();
    this.editingUniverseId = universe.id;
    this.nameInput = universe.name;
    this.descriptionInput = universe.description;
    this.coverImageInput = universe.cover_image;
    this.modal.set('edit');
  }

  openDelete(universe: UniverseWithStats): void {
    this.store.clearError();
    this.pendingDelete.set(universe);
    this.deleteConfirmation = '';
    this.modal.set('delete');
  }

  closeModal(): void {
    if (this.store.busy()) return;
    this.modal.set(null);
    this.pendingDelete.set(null);
  }

  async onCoverSelected(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !file.type.startsWith('image/') || file.size > 8 * 1024 * 1024) return;
    this.coverImageInput = await fileToDataUrl(file);
  }

  async save(): Promise<void> {
    const name = this.nameInput.trim();
    if (!name) return;
    if (this.editingUniverseId) {
      const saved = await this.store.update(this.editingUniverseId, {
        name,
        description: this.descriptionInput.trim(),
        coverImage: this.coverImageInput,
      });
      if (saved) { this.modal.set(null); this.updated.emit(); }
      return;
    }
    const created = await this.store.create({ name, description: this.descriptionInput.trim(), coverImage: this.coverImageInput });
    if (!created) return;
    this.modal.set(null);
    this.opened.emit(created);
  }

  async confirmDelete(): Promise<void> {
    const universe = this.pendingDelete();
    if (!universe || this.deleteConfirmation.trim() !== universe.name) return;
    if (await this.store.delete(universe.id)) {
      this.pendingDelete.set(null);
      this.modal.set(null);
      this.deleted.emit(universe.id);
    }
  }
}
