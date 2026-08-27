import { CommonModule } from '@angular/common';
import { Component, EventEmitter, Input, OnChanges, Output, SimpleChanges, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Entity } from '../../core/models';
import { ConnectionsGraphComponent } from './connections-graph.component';
import { ConnectionsStore } from './state/connections.store';

@Component({
  selector: 'app-connections-page',
  standalone: true,
  imports: [CommonModule, FormsModule, ConnectionsGraphComponent],
  templateUrl: './connections-page.component.html',
  styleUrl: './connections-page.component.css',
})
export class ConnectionsPageComponent implements OnChanges {
  @Input({ required: true }) universeId = '';
  @Input() entities: Entity[] = [];

  @Output() readonly entityOpenRequested = new EventEmitter<Entity>();
  @Output() readonly info = new EventEmitter<string>();
  @Output() readonly failed = new EventEmitter<string>();

  readonly store = inject(ConnectionsStore);

  readonly showNewRelation = signal(false);
  readonly pendingDelete = signal<{ id: string; label: string } | null>(null);

  newRelationSource = '';
  newRelationTarget = '';
  newRelationLabel = '';

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['universeId']) void this.store.load(this.universeId);
  }

  openEntity(entity: Entity): void { this.entityOpenRequested.emit(entity); }

  openCreateRelation(): void {
    this.newRelationSource = '';
    this.newRelationTarget = '';
    this.newRelationLabel = '';
    this.showNewRelation.set(true);
  }

  closeCreateRelation(): void { this.showNewRelation.set(false); }

  async createRelation(): Promise<void> {
    if (this.newRelationSource === this.newRelationTarget) { this.info.emit('Escolha duas entidades diferentes.'); return; }
    const created = await this.store.create(this.universeId, this.newRelationSource, this.newRelationTarget, this.newRelationLabel);
    if (!created) { this.reportStoreError('Não foi possível criar a conexão.'); return; }
    this.showNewRelation.set(false);
  }

  requestDelete(id: string, label: string): void {
    this.pendingDelete.set({ id, label });
  }

  async confirmDelete(): Promise<void> {
    const pending = this.pendingDelete();
    if (!pending) return;
    const ok = await this.store.delete(pending.id);
    if (!ok) { this.reportStoreError(`Não foi possível excluir ${pending.label}.`); return; }
    this.pendingDelete.set(null);
    this.info.emit('Ligação excluída do banco local.');
  }

  private reportStoreError(fallback: string): void {
    this.failed.emit(this.store.error() || fallback);
  }
}
