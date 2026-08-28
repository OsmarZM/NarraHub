import { Injectable, computed, inject } from '@angular/core';
import { AppState } from '../core/state/app.state';
import { EntityStore } from '../features/entities/state/entity.store';
import { ManuscriptStore } from '../features/manuscript/state/manuscript.store';
import { PlanningStore } from '../features/planning/state/planning.store';
import { TimelineStore } from '../features/timeline/state/timeline.store';
import { ShellState } from '../shell/state/shell.state';

export interface GlobalSearchResult {
  id: string;
  kind: 'story' | 'book' | 'chapter' | 'entity' | 'timeline' | 'planning';
  label: string;
  context: string;
  icon: string;
}

/**
 * Busca do cabeçalho: cruza cinco domínios, então não pertence a nenhum deles
 * nem ao layout — mora na camada de aplicação, junto de WorkspaceSyncService.
 *
 * Só lê dos stores. Quem decide o que fazer com o resultado escolhido é o
 * layout, que é quem sabe navegar sem brigar com o restoreRoute().
 *
 * É esta busca que justifica o pré-carregamento dos cinco domínios ao abrir um
 * universo: ela precisa achar um capítulo que o usuário nunca visitou. Na Fase
 * 4 isso vira um índice próprio e o pré-carregamento cai.
 */
@Injectable({ providedIn: 'root' })
export class GlobalSearchService {
  private readonly appState = inject(AppState);
  private readonly shell = inject(ShellState);
  private readonly manuscriptStore = inject(ManuscriptStore);
  private readonly entityStore = inject(EntityStore);
  private readonly timelineStore = inject(TimelineStore);
  private readonly planningStore = inject(PlanningStore);

  readonly query = this.shell.searchQuery;

  readonly results = computed<GlobalSearchResult[]>(() => {
    if (this.appState.currentView() !== 'workspace') return [];
    const query = this.normalize(this.query());
    if (query.length < 2) return [];
    const matches = (value: string) => this.normalize(value).includes(query);
    const results: GlobalSearchResult[] = [];
    for (const story of this.manuscriptStore.stories()) if (matches(`${story.name} ${story.description}`)) results.push({ id: story.id, kind: 'story', label: story.name, context: 'História', icon: '⌂' });
    for (const book of this.manuscriptStore.universeBooks()) if (matches(`${book.name} ${book.description} ${book.story_name}`)) results.push({ id: book.id, kind: 'book', label: book.name, context: `Livro · ${book.story_name}`, icon: '▱' });
    for (const chapter of this.manuscriptStore.universeChapters()) if (matches(`${chapter.title} ${chapter.content} ${chapter.book_name} ${chapter.story_name}`)) results.push({ id: chapter.id, kind: 'chapter', label: chapter.title, context: `${chapter.story_name} · ${chapter.book_name}`, icon: '▤' });
    for (const entity of this.entityStore.entities()) if (matches(`${entity.name} ${entity.summary} ${entity.description} ${entity.type}`)) results.push({ id: entity.id, kind: 'entity', label: entity.name, context: entity.type, icon: entity.name.charAt(0).toUpperCase() });
    for (const event of this.timelineStore.events()) if (matches(`${event.title} ${event.description} ${event.display_date || event.start_date}`)) results.push({ id: event.id, kind: 'timeline', label: event.title, context: 'Linha do tempo', icon: '◷' });
    for (const item of this.planningStore.items()) if (matches(`${item.title} ${item.description} ${item.status}`)) results.push({ id: item.id, kind: 'planning', label: item.title, context: `Planejamento · ${item.status}`, icon: '☑' });
    return results.slice(0, 24);
  });

  /** Ignora acento e caixa: "Anao" tem que achar "Anão". */
  private normalize(value: string): string {
    return value.normalize('NFD').replace(/[\u0300-\u036f]/gu, '').toLocaleLowerCase('pt-BR').trim();
  }
}
