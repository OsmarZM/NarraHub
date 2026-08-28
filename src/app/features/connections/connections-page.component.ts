import { CommonModule } from '@angular/common';
import { Component, EventEmitter, Input, OnChanges, Output, SimpleChanges, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { CanvasNode, CanvasNodeKind, Entity } from '../../core/models';
import { fileToDataUrl } from '../../shared/utils/file-to-data-url';
import { CanvasConnectRequest, CanvasPositionChange, ConnectionsGraphComponent } from './connections-graph.component';
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
  /** Par escolhido no canvas esperando o rótulo da ligação. */
  readonly pendingConnection = signal<CanvasConnectRequest | null>(null);
  readonly editingNode = signal<CanvasNode | null>(null);
  readonly pendingNodeDelete = signal<CanvasNode | null>(null);

  newRelationSource = '';
  newRelationTarget = '';
  newRelationLabel = '';
  connectionLabel = '';
  nodeText = '';

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['universeId']) void this.store.load(this.universeId);
  }

  openEntity(entity: Entity): void { this.entityOpenRequested.emit(entity); }

  // ── Relações canônicas ──────────────────────────

  openCreateRelation(): void {
    this.newRelationSource = '';
    this.newRelationTarget = '';
    this.newRelationLabel = '';
    this.showNewRelation.set(true);
  }

  closeCreateRelation(): void { this.showNewRelation.set(false); }

  async createRelation(): Promise<void> {
    if (this.newRelationSource === this.newRelationTarget) { this.info.emit('Escolha duas entidades diferentes.'); return; }
    if (!await this.store.create(this.universeId, this.newRelationSource, this.newRelationTarget, this.newRelationLabel)) {
      this.reportStoreError('Não foi possível criar a conexão.');
      return;
    }
    this.showNewRelation.set(false);
  }

  requestDelete(id: string, label: string): void { this.pendingDelete.set({ id, label }); }

  async confirmDelete(): Promise<void> {
    const pending = this.pendingDelete();
    if (!pending) return;
    if (!await this.store.delete(pending.id)) { this.reportStoreError(`Não foi possível excluir ${pending.label}.`); return; }
    this.pendingDelete.set(null);
    this.info.emit('Ligação excluída do banco local.');
  }

  // ── Elementos livres do canvas ──────────────────

  async addTitle(): Promise<void> { await this.addNode('title', 'Novo título'); }
  async addNote(): Promise<void> { await this.addNode('note', 'Nova nota'); }

  async onImageSelected(event: Event): Promise<void> {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    if (!file.type.startsWith('image/')) { this.info.emit('Escolha um arquivo de imagem.'); return; }
    if (file.size > 8 * 1024 * 1024) { this.info.emit('A imagem deve ter no máximo 8 MB.'); return; }
    await this.addNode('image', file.name.replace(/\.[^.]+$/u, ''), await fileToDataUrl(file));
  }

  /**
   * Solta o elemento novo perto do centro do que já existe, com um leve
   * espalhamento: cair sempre em (0,0) esconderia o item atrás do que já está lá.
   */
  private async addNode(kind: CanvasNodeKind, text: string, image = ''): Promise<void> {
    const count = this.store.canvasNodes().length;
    const created = await this.store.addNode(kind, text, image, 40 + (count % 5) * 60, 40 + Math.floor(count / 5) * 80);
    if (!created) { this.reportStoreError('Não foi possível adicionar o elemento.'); return; }
    this.info.emit(kind === 'title' ? 'Título adicionado ao canvas.' : kind === 'image' ? 'Imagem adicionada ao canvas.' : 'Nota adicionada ao canvas.');
  }

  openNodeEditor(node: CanvasNode): void {
    this.nodeText = node.text;
    this.editingNode.set(node);
  }

  async confirmNodeRename(): Promise<void> {
    const node = this.editingNode();
    if (!node) return;
    if (!await this.store.renameNode(node.id, this.nodeText)) { this.reportStoreError('Não foi possível renomear o elemento.'); return; }
    this.editingNode.set(null);
  }

  requestNodeDelete(node: CanvasNode): void { this.pendingNodeDelete.set(node); }

  async confirmNodeDelete(): Promise<void> {
    const node = this.pendingNodeDelete();
    if (!node) return;
    if (!await this.store.deleteNode(node.id)) { this.reportStoreError('Não foi possível excluir o elemento.'); return; }
    this.pendingNodeDelete.set(null);
    this.editingNode.set(null);
    this.info.emit('Elemento removido do canvas.');
  }

  // ── Layout e ligações do canvas ─────────────────

  onPositionChanged(change: CanvasPositionChange): void {
    void this.store.savePosition(change.kind, change.id, change.x, change.y);
  }

  onConnectRequested(request: CanvasConnectRequest): void {
    this.connectionLabel = '';
    this.pendingConnection.set(request);
  }

  async confirmConnection(): Promise<void> {
    const pending = this.pendingConnection();
    if (!pending) return;
    if (!await this.store.addEdge(pending.source, pending.target, this.connectionLabel)) {
      this.reportStoreError('Não foi possível criar a ligação.');
      return;
    }
    this.pendingConnection.set(null);
    this.info.emit('Ligação criada no canvas.');
  }

  async resetLayout(): Promise<void> {
    if (!await this.store.resetLayout()) { this.reportStoreError('Não foi possível recomeçar o layout.'); return; }
    this.info.emit('Layout salvo descartado — use "Organizar grafo" para um novo arranjo.');
  }

  async deleteCanvasEdge(id: string): Promise<void> {
    if (!await this.store.deleteEdge(id)) this.reportStoreError('Não foi possível excluir a ligação.');
  }

  /** Nome legível de uma ponta, para listar as ligações do canvas fora do grafo. */
  endpointLabel(kind: string, id: string): string {
    if (kind === 'entity') return this.entities.find((entity) => entity.id === id)?.name ?? 'entidade removida';
    const node = this.store.canvasNodes().find((item) => item.id === id);
    return node?.text || (node?.kind === 'title' ? 'Título' : node?.kind === 'image' ? 'Imagem' : 'Nota');
  }

  private reportStoreError(fallback: string): void {
    this.failed.emit(this.store.error() || fallback);
  }
}
