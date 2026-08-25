import { Component, computed, input, output, signal } from '@angular/core';
import { ContentTag, UniverseWithStats } from '../../core/models';

type UniverseSort = 'recent' | 'name' | 'created';

@Component({
  selector: 'app-universe-picker',
  standalone: true,
  templateUrl: './universe-picker.component.html',
  styleUrl: './universe-picker.component.css',
})
export class UniversePickerComponent {
  readonly universes = input.required<UniverseWithStats[]>();
  readonly query = input('');
  readonly lastOpenedId = input<string | null>(null);
  readonly tagsByUniverse = input<Record<string, ContentTag[]>>({});
  readonly createRequested = output<void>();
  readonly openRequested = output<UniverseWithStats>();
  readonly editRequested = output<UniverseWithStats>();
  readonly deleteRequested = output<UniverseWithStats>();

  readonly sort = signal<UniverseSort>('recent');
  readonly openMenuId = signal<string | null>(null);

  readonly visibleUniverses = computed(() => {
    const query = this.query().trim().toLocaleLowerCase('pt-BR');
    const result = this.universes().filter((universe) =>
      !query || `${universe.name} ${universe.description}`.toLocaleLowerCase('pt-BR').includes(query),
    );
    return [...result].sort((left, right) => {
      if (this.sort() === 'name') return left.name.localeCompare(right.name, 'pt-BR');
      if (this.sort() === 'created') return this.timestamp(right.created_at) - this.timestamp(left.created_at);
      return this.timestamp(right.updated_at) - this.timestamp(left.updated_at);
    });
  });

  toggleMenu(universeId: string, event: Event): void {
    event.stopPropagation();
    this.openMenuId.update((current) => current === universeId ? null : universeId);
  }

  chooseEdit(universe: UniverseWithStats, event: Event): void {
    event.stopPropagation();
    this.openMenuId.set(null);
    this.editRequested.emit(universe);
  }

  chooseDelete(universe: UniverseWithStats, event: Event): void {
    event.stopPropagation();
    this.openMenuId.set(null);
    this.deleteRequested.emit(universe);
  }

  characterCount(universe: UniverseWithStats): number {
    return universe.stats.entity_counts['Personagem'] ?? 0;
  }

  universeTags(universeId: string): ContentTag[] {
    return this.tagsByUniverse()[`universe:${universeId}`] ?? [];
  }

  relativeDate(value: string): string {
    const timestamp = this.timestamp(value);
    if (!timestamp) return 'Sem edição';
    const minutes = Math.max(0, Math.floor((Date.now() - timestamp) / 60_000));
    if (minutes < 2) return 'Editado agora';
    if (minutes < 60) return `Editado há ${minutes} min`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `Editado há ${hours} h`;
    const days = Math.floor(hours / 24);
    if (days < 30) return `Editado há ${days} dia${days === 1 ? '' : 's'}`;
    return new Date(timestamp).toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' });
  }

  coverStyle(universe: UniverseWithStats): Record<string, string> {
    return universe.cover_image ? { 'background-image': `url("${universe.cover_image}")` } : {};
  }

  private timestamp(value: string): number {
    if (!value) return 0;
    const normalized = value.includes('T') ? value : value.replace(' ', 'T') + 'Z';
    const timestamp = new Date(normalized).getTime();
    return Number.isNaN(timestamp) ? 0 : timestamp;
  }
}
