import { AfterViewInit, Component, ElementRef, Input, OnChanges, OnDestroy, Output, EventEmitter, SimpleChanges, ViewChild } from '@angular/core';
import type { Core, ElementDefinition } from 'cytoscape';
import { CanvasEdge, CanvasNode, Entity, ENTITY_SHAPES, RelationCard } from '../../core/models';

export interface CanvasPositionChange {
  kind: 'entity' | 'canvas';
  id: string;
  x: number;
  y: number;
}

export interface CanvasConnectRequest {
  source: { kind: 'entity' | 'canvas'; id: string; label: string };
  target: { kind: 'entity' | 'canvas'; id: string; label: string };
}

/** Prefixo nos ids do cytoscape para não colidir id de entidade com id de elemento livre. */
const CANVAS_PREFIX = 'canvas:';

@Component({
  selector: 'app-connections-graph',
  standalone: true,
  templateUrl: './connections-graph.component.html',
  styleUrl: './connections-graph.component.css',
})
export class ConnectionsGraphComponent implements AfterViewInit, OnChanges, OnDestroy {
  @Input({ required: true }) entities: Entity[] = [];
  @Input({ required: true }) relations: RelationCard[] = [];
  @Input() canvasNodes: CanvasNode[] = [];
  @Input() canvasEdges: CanvasEdge[] = [];
  @Input() entityPositions: Record<string, { x: number; y: number }> = {};

  @Output() entityOpened = new EventEmitter<Entity>();
  @Output() positionChanged = new EventEmitter<CanvasPositionChange>();
  @Output() connectRequested = new EventEmitter<CanvasConnectRequest>();
  @Output() canvasNodeActivated = new EventEmitter<CanvasNode>();
  @Output() canvasNodeDeleted = new EventEmitter<CanvasNode>();
  @Output() canvasEdgeDeleted = new EventEmitter<string>();

  @ViewChild('graphHost', { static: true }) private graphHost!: ElementRef<HTMLDivElement>;

  selectedEntity: Entity | null = null;
  selectedCanvasNode: CanvasNode | null = null;
  allTypes: string[] = [];
  visibleTypes = new Set<string>();

  /** Modo de ligação: primeiro clique escolhe a origem, segundo fecha a ligação. */
  connecting = false;
  private pendingSource: { kind: 'entity' | 'canvas'; id: string; label: string } | null = null;
  get connectHint(): string {
    if (!this.connecting) return '';
    return this.pendingSource ? `Ligando de “${this.pendingSource.label}” — clique no destino.` : 'Clique no primeiro elemento.';
  }

  private graph: Core | null = null;
  private resizeObserver: ResizeObserver | null = null;
  private themeObserver: MutationObserver | null = null;

  async ngAfterViewInit(): Promise<void> {
    this.syncTypes();
    await this.createGraph();
  }

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['entities']) this.syncTypes();
    if (this.graph) this.rebuildGraph();
  }

  ngOnDestroy(): void {
    this.resizeObserver?.disconnect();
    this.themeObserver?.disconnect();
    this.graph?.destroy();
  }

  toggleType(type: string): void {
    const next = new Set(this.visibleTypes);
    if (next.has(type)) next.delete(type); else next.add(type);
    this.visibleTypes = next;
    this.rebuildGraph();
  }

  toggleConnectMode(): void {
    this.connecting = !this.connecting;
    this.pendingSource = null;
    this.graph?.elements().removeClass('connect-source');
  }

  cancelConnect(): void {
    this.connecting = false;
    this.pendingSource = null;
    this.graph?.elements().removeClass('connect-source');
  }

  /**
   * Roda o arranjo automático e persiste o resultado: depois que existe layout
   * salvo o grafo nunca mais se reorganiza sozinho, então "Organizar" precisa
   * ser uma ação explícita e definitiva, não um efeito colateral de recarregar.
   */
  organize(): void {
    if (!this.graph) return;
    const layout = this.graph.layout({ name: 'cose', animate: true, animationDuration: 420, fit: true, padding: 54 });
    layout.one('layoutstop', () => this.emitAllPositions());
    layout.run();
  }

  fit(): void { this.graph?.fit(undefined, 48); }
  zoomIn(): void { if (this.graph) this.graph.zoom({ level: Math.min(2.5, this.graph.zoom() * 1.18), renderedPosition: { x: this.graph.width() / 2, y: this.graph.height() / 2 } }); }
  zoomOut(): void { if (this.graph) this.graph.zoom({ level: Math.max(.35, this.graph.zoom() / 1.18), renderedPosition: { x: this.graph.width() / 2, y: this.graph.height() / 2 } }); }

  focusSelected(): void {
    if (!this.graph || !this.selectedEntity) return;
    const node = this.graph.getElementById(this.selectedEntity.id);
    const neighborhood = node.closedNeighborhood();
    this.graph.elements().addClass('dimmed');
    neighborhood.removeClass('dimmed');
    this.graph.animate({ fit: { eles: neighborhood, padding: 80 }, duration: 360 });
  }

  clearFocus(): void {
    this.graph?.elements().removeClass('dimmed');
    this.selectedEntity = null;
    this.selectedCanvasNode = null;
    this.fit();
  }

  relationsFor(entity: Entity): RelationCard[] {
    return this.relations.filter((relation) => relation.source_id === entity.id || relation.target_id === entity.id);
  }

  otherName(relation: RelationCard, entity: Entity): string {
    return relation.source_id === entity.id ? relation.target_name : relation.source_name;
  }

  private async createGraph(): Promise<void> {
    const { default: cytoscape } = await import('cytoscape');
    this.graph = cytoscape({
      container: this.graphHost.nativeElement,
      elements: this.elements(),
      minZoom: .25,
      maxZoom: 3,
      wheelSensitivity: .18,
      layout: this.layoutOptions(),
      style: [
        { selector: 'node', style: {
          'shape': 'data(shape)', 'width': 76, 'height': 76, 'background-color': 'data(color)',
          'border-width': 4, 'border-color': '#fffaf2', 'outline-width': 2, 'outline-color': 'data(color)',
          'label': 'data(label)', 'font-size': 12, 'font-weight': 700, 'color': '#10213a',
          'text-valign': 'bottom', 'text-margin-y': 12, 'text-wrap': 'wrap', 'text-max-width': 120,
          'overlay-opacity': 0, 'transition-property': 'opacity, border-width', 'transition-duration': .18,
        } },
        { selector: 'node[hasImage = 1]', style: { 'background-image': 'data(image)', 'background-fit': 'cover', 'background-clip': 'node' } },
        // Elementos livres: sem contorno de "ficha", texto dentro, visual de anotação.
        { selector: 'node[group = "canvas"]', style: {
          'border-width': 2, 'border-style': 'dashed', 'text-valign': 'center', 'text-margin-y': 0,
          'font-weight': 600, 'text-max-width': 150,
        } },
        { selector: 'node[kind = "title"]', style: {
          'shape': 'round-rectangle', 'width': 190, 'height': 52, 'font-size': 17, 'font-weight': 800,
          'border-width': 0, 'text-max-width': 170,
        } },
        { selector: 'node[kind = "note"]', style: { 'shape': 'round-rectangle', 'width': 160, 'height': 96, 'font-size': 11 } },
        { selector: 'node[kind = "image"]', style: {
          'shape': 'round-rectangle', 'width': 132, 'height': 132, 'border-style': 'solid',
          'text-valign': 'bottom', 'text-margin-y': 10, 'font-size': 11,
        } },
        { selector: 'edge', style: {
          'curve-style': 'bezier', 'width': 2, 'line-color': '#9f8b84', 'target-arrow-color': '#6b173d',
          'target-arrow-shape': 'triangle', 'arrow-scale': .8, 'label': 'data(label)', 'font-size': 10,
          'color': '#4c102f', 'text-background-color': '#fffaf2', 'text-background-opacity': .94,
          'text-background-padding': 5, 'text-rotation': 'autorotate', 'overlay-opacity': 0,
        } },
        // Ligação de diagrama é anotação, não cânone — tracejada para não se
        // confundir com uma relação de verdade da ficha.
        { selector: 'edge[group = "canvas"]', style: { 'line-style': 'dashed', 'width': 2, 'target-arrow-shape': 'vee' } },
        { selector: 'edge[bidirectional = 1]', style: { 'source-arrow-shape': 'triangle', 'source-arrow-color': '#6b173d' } },
        { selector: ':selected', style: { 'border-width': 7, 'line-color': '#c9683d', 'target-arrow-color': '#c9683d' } },
        { selector: '.connect-source', style: { 'border-width': 8, 'border-color': '#c9683d', 'border-style': 'solid' } },
        { selector: '.dimmed', style: { 'opacity': .12 } },
      ] as any,
    });

    this.graph.on('tap', 'node', (event) => this.onNodeTap(event.target.id()));
    this.graph.on('dbltap', 'node', (event) => {
      const ref = this.parseId(event.target.id());
      if (ref.kind === 'entity') {
        const entity = this.entities.find((item) => item.id === ref.id);
        if (entity) this.entityOpened.emit(entity);
      } else {
        const node = this.canvasNodes.find((item) => item.id === ref.id);
        if (node) this.canvasNodeActivated.emit(node);
      }
    });
    this.graph.on('dragfree', 'node', (event) => {
      const ref = this.parseId(event.target.id());
      const position = event.target.position();
      this.positionChanged.emit({ kind: ref.kind, id: ref.id, x: Math.round(position.x), y: Math.round(position.y) });
    });
    this.graph.on('tap', (event) => {
      if (event.target !== this.graph) return;
      this.selectedEntity = null;
      this.selectedCanvasNode = null;
    });

    this.applyTheme();
    this.themeObserver = new MutationObserver(() => this.applyTheme());
    this.themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    this.resizeObserver = new ResizeObserver(() => { this.graph?.resize(); });
    this.resizeObserver.observe(this.graphHost.nativeElement);
  }

  private onNodeTap(rawId: string): void {
    const ref = this.parseId(rawId);
    if (this.connecting) {
      const label = this.labelFor(ref);
      if (!this.pendingSource) {
        this.pendingSource = { ...ref, label };
        this.graph?.getElementById(rawId).addClass('connect-source');
        return;
      }
      if (this.pendingSource.kind === ref.kind && this.pendingSource.id === ref.id) return;
      this.connectRequested.emit({ source: this.pendingSource, target: { ...ref, label } });
      this.cancelConnect();
      return;
    }
    if (ref.kind === 'entity') {
      this.selectedEntity = this.entities.find((entity) => entity.id === ref.id) ?? null;
      this.selectedCanvasNode = null;
    } else {
      this.selectedCanvasNode = this.canvasNodes.find((node) => node.id === ref.id) ?? null;
      this.selectedEntity = null;
    }
  }

  private rebuildGraph(): void {
    if (!this.graph) return;
    this.graph.elements().remove();
    this.graph.add(this.elements());
    // Só reorganiza sozinho quando ainda não existe layout salvo — depois disso
    // o arranjo é do usuário, e mexer nele a cada recarga seria perder o trabalho.
    if (this.hasSavedLayout()) this.graph.fit(undefined, 48);
    else this.graph.layout({ name: 'cose', animate: false, fit: true, padding: 54 }).run();
  }

  private hasSavedLayout(): boolean {
    return Object.keys(this.entityPositions).length > 0 || this.canvasNodes.length > 0;
  }

  private layoutOptions(): any {
    return this.hasSavedLayout()
      ? { name: 'preset', fit: true, padding: 54 }
      : { name: 'cose', animate: false, fit: true, padding: 54 };
  }

  private elements(): ElementDefinition[] {
    const visible = this.entities.filter((entity) => this.visibleTypes.has(entity.type));
    const entityIds = new Set(visible.map((entity) => entity.id));
    let unplaced = 0;

    const entityNodes: ElementDefinition[] = visible.map((entity) => {
      const visual = ENTITY_SHAPES[entity.type] ?? { shape: 'round-rectangle', color: '#8b7f78' };
      const saved = this.entityPositions[entity.id];
      return {
        data: { id: entity.id, group: 'entity', label: entity.name, type: entity.type, shape: visual.shape,
                color: visual.color, image: entity.image, hasImage: entity.image ? 1 : 0 },
        position: saved ?? this.fallbackPosition(unplaced++),
      };
    });

    const canvasNodes: ElementDefinition[] = this.canvasNodes.map((node) => ({
      data: {
        id: CANVAS_PREFIX + node.id, group: 'canvas', kind: node.kind,
        label: node.kind === 'image' ? node.text : (node.text || this.placeholderFor(node.kind)),
        shape: 'round-rectangle', color: node.color || this.defaultColorFor(node.kind),
        image: node.image, hasImage: node.image ? 1 : 0,
      },
      position: { x: node.position_x, y: node.position_y },
    }));

    const canvasNodeIds = new Set(this.canvasNodes.map((node) => node.id));
    const exists = (kind: string, id: string) => kind === 'entity' ? entityIds.has(id) : canvasNodeIds.has(id);

    const relationEdges: ElementDefinition[] = this.relations
      .filter((relation) => entityIds.has(relation.source_id) && entityIds.has(relation.target_id))
      .map((relation) => ({ data: { id: relation.id, group: 'relation', source: relation.source_id,
        target: relation.target_id, label: relation.label, bidirectional: relation.bidirectional ? 1 : 0 } }));

    const diagramEdges: ElementDefinition[] = this.canvasEdges
      .filter((edge) => exists(edge.source_kind, edge.source_id) && exists(edge.target_kind, edge.target_id))
      .map((edge) => ({ data: {
        id: CANVAS_PREFIX + edge.id, group: 'canvas',
        source: this.cyId(edge.source_kind, edge.source_id),
        target: this.cyId(edge.target_kind, edge.target_id),
        label: edge.label,
      } }));

    return [...entityNodes, ...canvasNodes, ...relationEdges, ...diagramEdges];
  }

  /** Espalha em grade quem ainda não tem posição, em vez de empilhar tudo na origem. */
  private fallbackPosition(index: number): { x: number; y: number } {
    const columns = 6;
    return { x: (index % columns) * 190, y: Math.floor(index / columns) * 190 };
  }

  private emitAllPositions(): void {
    this.graph?.nodes().forEach((node) => {
      const ref = this.parseId(node.id());
      const position = node.position();
      this.positionChanged.emit({ kind: ref.kind, id: ref.id, x: Math.round(position.x), y: Math.round(position.y) });
    });
  }

  private cyId(kind: string, id: string): string {
    return kind === 'canvas' ? CANVAS_PREFIX + id : id;
  }

  private parseId(rawId: string): { kind: 'entity' | 'canvas'; id: string } {
    return rawId.startsWith(CANVAS_PREFIX)
      ? { kind: 'canvas', id: rawId.slice(CANVAS_PREFIX.length) }
      : { kind: 'entity', id: rawId };
  }

  private labelFor(ref: { kind: 'entity' | 'canvas'; id: string }): string {
    if (ref.kind === 'entity') return this.entities.find((entity) => entity.id === ref.id)?.name ?? 'elemento';
    const node = this.canvasNodes.find((item) => item.id === ref.id);
    return node?.text || this.placeholderFor(node?.kind ?? 'note');
  }

  private placeholderFor(kind: string): string {
    return kind === 'title' ? 'Título' : kind === 'image' ? 'Imagem' : 'Nota';
  }

  private defaultColorFor(kind: string): string {
    return kind === 'title' ? '#6b173d' : kind === 'image' ? '#8b7f78' : '#c9a227';
  }

  private syncTypes(): void {
    const types = new Set(this.entities.map((entity) => entity.type));
    const previousTypes = new Set(this.allTypes);
    this.allTypes = [...types].sort((a, b) => a.localeCompare(b, 'pt-BR'));
    if (!previousTypes.size) this.visibleTypes = new Set(types);
    else {
      const next = new Set([...this.visibleTypes].filter((type) => types.has(type)));
      for (const type of types) if (!previousTypes.has(type)) next.add(type);
      this.visibleTypes = next;
    }
  }

  private applyTheme(): void {
    if (!this.graph) return;
    const css = getComputedStyle(document.documentElement);
    const ink = css.getPropertyValue('--nh-ink').trim();
    const muted = css.getPropertyValue('--nh-ink-muted').trim();
    const surface = css.getPropertyValue('--nh-surface').trim();
    const wine = css.getPropertyValue('--nh-wine').trim();
    const coral = css.getPropertyValue('--nh-coral').trim();
    const style = this.graph.style() as any;
    style.selector('node').style({ 'border-color': surface, color: ink });
    style.selector('node[group = "canvas"]').style({ 'border-color': muted, color: surface });
    style.selector('edge').style({ 'line-color': muted, 'target-arrow-color': wine, color: wine, 'text-background-color': surface });
    style.selector('edge[bidirectional = 1]').style({ 'source-arrow-color': wine });
    style.selector(':selected').style({ 'line-color': coral, 'target-arrow-color': coral });
    style.selector('.connect-source').style({ 'border-color': coral });
    style.update();
  }
}
