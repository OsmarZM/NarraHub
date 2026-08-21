import { AfterViewInit, Component, ElementRef, Input, OnChanges, OnDestroy, Output, EventEmitter, SimpleChanges, ViewChild } from '@angular/core';
import type { Core, ElementDefinition } from 'cytoscape';
import { Entity, ENTITY_SHAPES, RelationCard } from '../../core/models';

@Component({
  selector: 'app-connections-graph',
  standalone: true,
  templateUrl: './connections-graph.component.html',
  styleUrl: './connections-graph.component.css',
})
export class ConnectionsGraphComponent implements AfterViewInit, OnChanges, OnDestroy {
  @Input({ required: true }) entities: Entity[] = [];
  @Input({ required: true }) relations: RelationCard[] = [];
  @Output() entityOpened = new EventEmitter<Entity>();
  @ViewChild('graphHost', { static: true }) private graphHost!: ElementRef<HTMLDivElement>;

  selectedEntity: Entity | null = null;
  allTypes: string[] = [];
  visibleTypes = new Set<string>();
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

  organize(): void {
    this.graph?.layout({ name: 'cose', animate: true, animationDuration: 420, fit: true, padding: 54 }).run();
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
      layout: { name: 'cose', animate: false, fit: true, padding: 54 },
      style: [
        { selector: 'node', style: {
          'shape': 'data(shape)', 'width': 76, 'height': 76, 'background-color': 'data(color)',
          'border-width': 4, 'border-color': '#fffaf2', 'outline-width': 2, 'outline-color': 'data(color)',
          'label': 'data(label)', 'font-size': 12, 'font-weight': 700, 'color': '#10213a',
          'text-valign': 'bottom', 'text-margin-y': 12, 'text-wrap': 'wrap', 'text-max-width': 120,
          'overlay-opacity': 0, 'transition-property': 'opacity, border-width', 'transition-duration': .18,
        } },
        { selector: 'node[hasImage = 1]', style: { 'background-image': 'data(image)', 'background-fit': 'cover', 'background-clip': 'node' } },
        { selector: 'edge', style: {
          'curve-style': 'bezier', 'width': 2, 'line-color': '#9f8b84', 'target-arrow-color': '#6b173d',
          'target-arrow-shape': 'triangle', 'arrow-scale': .8, 'label': 'data(label)', 'font-size': 10,
          'color': '#4c102f', 'text-background-color': '#fffaf2', 'text-background-opacity': .94,
          'text-background-padding': 5, 'text-rotation': 'autorotate', 'overlay-opacity': 0,
        } },
        { selector: 'edge[bidirectional = 1]', style: { 'source-arrow-shape': 'triangle', 'source-arrow-color': '#6b173d' } },
        { selector: ':selected', style: { 'border-width': 7, 'line-color': '#c9683d', 'target-arrow-color': '#c9683d' } },
        { selector: '.dimmed', style: { 'opacity': .12 } },
      ] as any,
    });
    this.graph.on('tap', 'node', (event) => {
      this.selectedEntity = this.entities.find((entity) => entity.id === event.target.id()) ?? null;
    });
    this.graph.on('dbltap', 'node', (event) => {
      const entity = this.entities.find((item) => item.id === event.target.id());
      if (entity) this.entityOpened.emit(entity);
    });
    this.graph.on('tap', (event) => { if (event.target === this.graph) this.selectedEntity = null; });
    this.applyTheme();
    this.themeObserver = new MutationObserver(() => this.applyTheme());
    this.themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    this.resizeObserver = new ResizeObserver(() => { this.graph?.resize(); });
    this.resizeObserver.observe(this.graphHost.nativeElement);
  }

  private rebuildGraph(): void {
    if (!this.graph) return;
    this.graph.elements().remove();
    this.graph.add(this.elements());
    this.organize();
  }

  private elements(): ElementDefinition[] {
    const visible = this.entities.filter((entity) => this.visibleTypes.has(entity.type));
    const ids = new Set(visible.map((entity) => entity.id));
    const nodes: ElementDefinition[] = visible.map((entity) => {
      const visual = ENTITY_SHAPES[entity.type] ?? { shape: 'round-rectangle', color: '#8b7f78' };
      return { data: { id: entity.id, label: entity.name, type: entity.type, shape: visual.shape, color: visual.color, image: entity.image, hasImage: entity.image ? 1 : 0 } };
    });
    const edges: ElementDefinition[] = this.relations
      .filter((relation) => ids.has(relation.source_id) && ids.has(relation.target_id))
      .map((relation) => ({ data: { id: relation.id, source: relation.source_id, target: relation.target_id, label: relation.label, bidirectional: relation.bidirectional ? 1 : 0 } }));
    return [...nodes, ...edges];
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
    style.selector('edge').style({ 'line-color': muted, 'target-arrow-color': wine, color: wine, 'text-background-color': surface });
    style.selector('edge[bidirectional = 1]').style({ 'source-arrow-color': wine });
    style.selector(':selected').style({ 'line-color': coral, 'target-arrow-color': coral });
    style.update();
  }
}
