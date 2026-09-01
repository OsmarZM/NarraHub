import { CommonModule } from '@angular/common';
import { Component, OnInit, inject, signal } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';
import {
  ProductionReplicaCatalog,
  ProductionReplicaChapter,
  ProductionReplicaService,
  ProductionReplicaStatus,
  ReplicaCatalogItem,
} from '../../core/native/production-replica.service';

@Component({
  selector: 'app-production-replica',
  standalone: true,
  imports: [CommonModule],
  templateUrl: './production-replica.component.html',
  styleUrl: './production-replica.component.css',
})
export class ProductionReplicaComponent implements OnInit {
  private readonly replicaService = inject(ProductionReplicaService);

  readonly status = signal<ProductionReplicaStatus | null>(null);
  readonly catalog = signal<ProductionReplicaCatalog | null>(null);
  readonly chapter = signal<ProductionReplicaChapter | null>(null);
  readonly busy = signal(false);
  readonly error = signal('');

  async ngOnInit(): Promise<void> {
    if (!isTauri()) return;
    try {
      const status = await this.replicaService.status();
      this.status.set(status);
      if (status.enabled && status.sourceExists) await this.refresh(true);
    } catch (error) {
      this.error.set(error instanceof Error ? error.message : String(error));
    }
  }

  async refresh(_silent = false): Promise<void> {
    if (this.busy()) return;
    this.busy.set(true);
    this.error.set('');
    try {
      this.status.set(await this.replicaService.refresh());
      this.catalog.set(await this.replicaService.catalog());
    } catch (error) {
      this.error.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.busy.set(false);
    }
  }

  async loadCatalog(): Promise<void> {
    if (this.busy() || !this.status()?.snapshotId) return;
    this.busy.set(true);
    this.error.set('');
    try {
      this.catalog.set(await this.replicaService.catalog());
    } catch (error) {
      this.error.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.busy.set(false);
    }
  }

  async openChapter(chapterId: string): Promise<void> {
    if (this.busy()) return;
    this.busy.set(true);
    this.error.set('');
    try {
      this.chapter.set(await this.replicaService.chapter(chapterId));
    } catch (error) {
      this.error.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.busy.set(false);
    }
  }

  closeChapter(): void {
    this.chapter.set(null);
  }

  stories(universeId: string): ReplicaCatalogItem[] {
    return this.catalog()?.stories.filter((story) => story.parentId === universeId) || [];
  }

  books(storyId: string): ReplicaCatalogItem[] {
    return this.catalog()?.books.filter((book) => book.parentId === storyId) || [];
  }

  chapters(bookId: string): ReplicaCatalogItem[] {
    return this.catalog()?.chapters.filter((chapter) => chapter.parentId === bookId) || [];
  }

  changeKindLabel(kind: string): string {
    if (kind === 'universe') return 'Universo';
    if (kind === 'story') return 'História';
    if (kind === 'book') return 'Livro';
    if (kind === 'chapter') return 'Capítulo';
    return 'Entidade';
  }

  formatDate(value: string): string {
    if (!value) return '—';
    return new Intl.DateTimeFormat('pt-BR', {
      dateStyle: 'short',
      timeStyle: 'short',
    }).format(new Date(value));
  }
}
