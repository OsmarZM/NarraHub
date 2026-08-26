import { Component, Input, OnChanges, inject } from '@angular/core';
import { HistoryStore } from './state/history.store';

@Component({
  selector: 'app-history-page',
  standalone: true,
  templateUrl: './history-page.component.html',
  styleUrl: './history-page.component.css',
})
export class HistoryPageComponent implements OnChanges {
  @Input({ required: true }) universeId = '';
  readonly store = inject(HistoryStore);

  ngOnChanges(): void {
    void this.store.load(this.universeId);
  }

  formatDate(value: string): string {
    if (!value) return 'Sem data';
    const date = new Date(value.length === 10 ? `${value}T12:00:00` : value);
    return Number.isNaN(date.getTime())
      ? value
      : date.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' });
  }
}

