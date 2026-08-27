import { Injectable, inject } from '@angular/core';
import { Book, BookOption, Chapter, ChapterOption, Story } from '../../../core/models';
import { BookService } from '../../../core/services/book.service';
import { ChapterService } from '../../../core/services/chapter.service';
import { StoryService } from '../../../core/services/story.service';
import { ManuscriptGateway, UpdateBookInput, UpdateStoryInput } from './manuscript.gateway';

@Injectable({ providedIn: 'root' })
export class LegacyManuscriptGateway implements ManuscriptGateway {
  private readonly storyService = inject(StoryService);
  private readonly bookService = inject(BookService);
  private readonly chapterService = inject(ChapterService);

  listStories(universeId: string): Promise<Story[]> {
    return this.storyService.listByUniverse(universeId);
  }

  createStory(universeId: string, name: string): Promise<Story> {
    return this.storyService.create(universeId, name);
  }

  updateStory(id: string, patch: UpdateStoryInput): Promise<void> {
    return this.storyService.update(id, patch);
  }

  deleteStory(id: string): Promise<void> {
    return this.storyService.delete(id);
  }

  listBooksByStory(storyId: string): Promise<Book[]> {
    return this.bookService.listByStory(storyId);
  }

  listBooksByUniverse(universeId: string): Promise<BookOption[]> {
    return this.bookService.listByUniverse(universeId);
  }

  createBook(storyId: string, name: string): Promise<Book> {
    return this.bookService.create(storyId, name);
  }

  updateBook(id: string, patch: UpdateBookInput): Promise<void> {
    return this.bookService.update(id, { name: patch.name, description: patch.description, cover_image: patch.coverImage });
  }

  deleteBook(id: string): Promise<void> {
    return this.bookService.delete(id);
  }

  listChaptersByBook(bookId: string): Promise<Chapter[]> {
    return this.chapterService.listByBook(bookId);
  }

  listChaptersByUniverse(universeId: string): Promise<ChapterOption[]> {
    return this.chapterService.listByUniverse(universeId);
  }

  getChapter(id: string): Promise<Chapter | null> {
    return this.chapterService.get(id);
  }

  createChapter(bookId: string, title: string): Promise<Chapter> {
    return this.chapterService.create(bookId, title);
  }

  updateChapterTitle(id: string, title: string): Promise<void> {
    return this.chapterService.updateTitle(id, title);
  }

  updateChapterContent(id: string, content: string, wordCount: number): Promise<void> {
    return this.chapterService.updateContent(id, content, wordCount);
  }

  updateChapterSummary(id: string, summary: string): Promise<void> {
    return this.chapterService.updateSummary(id, summary);
  }

  updateChapterSceneRoute(id: string, sceneOrigin: string, sceneDestination: string): Promise<void> {
    return this.chapterService.updateSceneRoute(id, sceneOrigin, sceneDestination);
  }

  reorderChapters(bookId: string, chapterIds: string[]): Promise<void> {
    return this.chapterService.reorder(bookId, chapterIds);
  }

  deleteChapter(id: string): Promise<void> {
    return this.chapterService.delete(id);
  }
}
