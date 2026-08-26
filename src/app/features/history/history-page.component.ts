import { Component, Input, OnChanges, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { HistoryStore } from './state/history.store';

@Component({
  selector: 'app-history-page',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './history-page.component.html',
  styleUrl: './history-page.component.css',
})
export class HistoryPageComponent implements OnChanges {
  @Input({ required: true }) universeId = '';
  readonly store = inject(HistoryStore);
  readonly searchQuery = signal('');

  ngOnChanges(): void {
    void this.store.load(this.universeId);
  }

  filteredEntries() {
    const query = this.searchQuery().trim().toLowerCase();
    const entries = this.store.entries();
    if (!query) return entries;
    return entries.filter((entry) => {
      const matchName = (entry.display_name || '').toLowerCase().includes(query);
      const matchType = (entry.entity_type || '').toLowerCase().includes(query);
      const matchField = (entry.field || '').toLowerCase().includes(query);
      const matchVal = (entry.new_value || '').toLowerCase().includes(query);
      const matchAction = (entry.action || '').toLowerCase().includes(query);
      return matchName || matchType || matchField || matchVal || matchAction;
    });
  }

  actionLabel(action: string): string {
    switch (action?.toLowerCase()) {
      case 'create': return 'Criado';
      case 'update': return 'Editado';
      case 'delete': return 'Excluído';
      default: return action || 'Alteração';
    }
  }

  actionIcon(action: string): string {
    switch (action?.toLowerCase()) {
      case 'create': return '＋';
      case 'update': return '✎';
      case 'delete': return '×';
      default: return '↻';
    }
  }

  formatDate(value: string): string {
    if (!value) return 'Sem data';
    const date = new Date(value.length === 10 ? `${value}T12:00:00` : value);
    if (Number.isNaN(date.getTime())) return value;
    
    return date.toLocaleDateString('pt-BR', {
      day: '2-digit',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
    });
  }
}
