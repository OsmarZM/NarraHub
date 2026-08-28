import { Injectable, inject } from '@angular/core';
import { Book, BookOption, Chapter, ChapterOption, Story } from '../../../core/models';
import { RustCoreService } from '../../../core/services/rust-core.service';
import { ManuscriptGateway, UpdateBookInput, UpdateStoryInput } from './manuscript.gateway';

/**
 * Ordens 6 e 7 — história, livro, capítulo e autosave. Domínio migrado por
 * inteiro.
 *
 * Os quatro `updateChapter*` do contrato viram um comando só com um patch
 * parcial: `null` ali significa "esta tela não mexeu nisso". Sem isso, o
 * inspetor salvando o resumo sobrescreveria o texto que o editor acabou de
 * gravar.
 */
@Injectable({ providedIn: 'root' })
export class RustManuscriptGateway implements ManuscriptGateway {
  private readonly core = inject(RustCoreService);

  listStories(universeId: string): Promise<Story[]> {
    return this.core.call<Story[]>('story_list', { universeId });
  }

  createStory(universeId: string, name: string): Promise<Story> {
    return this.core.call<Story>('story_create', { universeId, name });
  }

  updateStory(id: string, patch: UpdateStoryInput): Promise<void> {
    return this.core.call<void>('story_update', { id, patch });
  }

  deleteStory(id: string): Promise<void> {
    return this.core.call<void>('story_delete', { id });
  }

  listBooksByStory(storyId: string): Promise<Book[]> {
    return this.core.call<Book[]>('book_list_by_story', { storyId });
  }

  listBooksByUniverse(universeId: string): Promise<BookOption[]> {
    return this.core.call<BookOption[]>('book_list_by_universe', { universeId });
  }

  createBook(storyId: string, name: string): Promise<Book> {
    return this.core.call<Book>('book_create', { storyId, name });
  }

  updateBook(id: string, patch: UpdateBookInput): Promise<void> {
    return this.core.call<void>('book_update', { id, patch });
  }

  deleteBook(id: string): Promise<void> {
    return this.core.call<void>('book_delete', { id });
  }

  listChaptersByBook(bookId: string): Promise<Chapter[]> {
    return this.core.call<Chapter[]>('chapter_list_by_book', { bookId });
  }

  listChaptersByUniverse(universeId: string): Promise<ChapterOption[]> {
    return this.core.call<ChapterOption[]>('chapter_list_by_universe', { universeId });
  }

  getChapter(id: string): Promise<Chapter | null> {
    return this.core.call<Chapter | null>('chapter_get', { id });
  }

  createChapter(bookId: string, title: string): Promise<Chapter> {
    return this.core.call<Chapter>('chapter_create', { bookId, title });
  }

  updateChapterTitle(id: string, title: string): Promise<void> {
    return this.patchChapter(id, { title });
  }

  updateChapterContent(id: string, content: string, wordCount: number): Promise<void> {
    // Texto e contagem sempre juntos: a estatística do universo soma
    // word_count, e o core recusa um sem o outro.
    return this.patchChapter(id, { content, wordCount });
  }

  updateChapterSummary(id: string, summary: string): Promise<void> {
    return this.patchChapter(id, { summary });
  }

  updateChapterSceneRoute(id: string, sceneOrigin: string, sceneDestination: string): Promise<void> {
    return this.patchChapter(id, { sceneOrigin, sceneDestination });
  }

  reorderChapters(bookId: string, chapterIds: string[]): Promise<void> {
    return this.core.call<void>('chapter_reorder', { bookId, chapterIds });
  }

  deleteChapter(id: string): Promise<void> {
    return this.core.call<void>('chapter_delete', { id });
  }

  private patchChapter(id: string, patch: Record<string, unknown>): Promise<void> {
    return this.core.call<void>('chapter_update', { id, patch });
  }
}
