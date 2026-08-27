import { Book, BookOption, Chapter, ChapterOption, Story } from '../../../core/models';

export interface UpdateStoryInput {
  name?: string;
  description?: string;
}

export interface UpdateBookInput {
  name?: string;
  description?: string;
  coverImage?: string;
}

export abstract class ManuscriptGateway {
  // Story
  abstract listStories(universeId: string): Promise<Story[]>;
  abstract createStory(universeId: string, name: string): Promise<Story>;
  abstract updateStory(id: string, patch: UpdateStoryInput): Promise<void>;
  abstract deleteStory(id: string): Promise<void>;

  // Book
  abstract listBooksByStory(storyId: string): Promise<Book[]>;
  abstract listBooksByUniverse(universeId: string): Promise<BookOption[]>;
  abstract createBook(storyId: string, name: string): Promise<Book>;
  abstract updateBook(id: string, patch: UpdateBookInput): Promise<void>;
  abstract deleteBook(id: string): Promise<void>;

  // Chapter
  abstract listChaptersByBook(bookId: string): Promise<Chapter[]>;
  abstract listChaptersByUniverse(universeId: string): Promise<ChapterOption[]>;
  abstract getChapter(id: string): Promise<Chapter | null>;
  abstract createChapter(bookId: string, title: string): Promise<Chapter>;
  abstract updateChapterTitle(id: string, title: string): Promise<void>;
  abstract updateChapterContent(id: string, content: string, wordCount: number): Promise<void>;
  abstract updateChapterSummary(id: string, summary: string): Promise<void>;
  abstract updateChapterSceneRoute(id: string, sceneOrigin: string, sceneDestination: string): Promise<void>;
  abstract reorderChapters(bookId: string, chapterIds: string[]): Promise<void>;
  abstract deleteChapter(id: string): Promise<void>;
}
