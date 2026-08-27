import { Injectable, inject, signal } from '@angular/core';
import { Book, BookOption, Chapter, ChapterOption, Story } from '../../../core/models';
import { ManuscriptGateway } from '../gateways/manuscript.gateway';

export interface SavedChapter {
  chapter: Chapter;
  contentChanged: boolean;
}

@Injectable({ providedIn: 'root' })
export class ManuscriptStore {
  private readonly gateway = inject(ManuscriptGateway);
  private requestedUniverseId = '';
  private loadRevision = 0;
  private saveTimer: ReturnType<typeof setTimeout> | null = null;

  readonly stories = signal<Story[]>([]);
  readonly universeBooks = signal<BookOption[]>([]);
  readonly universeChapters = signal<ChapterOption[]>([]);
  readonly books = signal<Book[]>([]);
  readonly chapters = signal<Chapter[]>([]);
  readonly activeStory = signal<Story | null>(null);
  readonly activeBook = signal<Book | null>(null);
  readonly activeChapter = signal<Chapter | null>(null);
  readonly expandedStoryIds = signal<Set<string>>(new Set());
  readonly expandedBookIds = signal<Set<string>>(new Set());
  readonly error = signal('');

  readonly editorTitle = signal('');
  readonly editorContent = signal('');
  readonly chapterSummary = signal('');
  readonly saveMessage = signal('');
  readonly isSaving = signal(false);
  readonly inspectorOpen = signal(localStorage.getItem('narrahub.inspectorOpen') !== 'false');

  /**
   * Hook cross-domain: a App usa isso para sincronizar menções (Knowledge,
   * ainda não extraído) e estatísticas do universo depois de um salvamento
   * bem-sucedido, sem o ManuscriptStore precisar conhecer os serviços de
   * menções ou de estatísticas do universo. Não é um padrão para repetir sem necessidade — stores não
   * têm @Output(); isso existe só porque App é a única consumidora.
   */
  onChapterPersisted: ((chapterId: string, content: string) => void) | null = null;

  async load(universeId: string): Promise<void> {
    const revision = ++this.loadRevision;
    this.requestedUniverseId = universeId;
    this.error.set('');
    try {
      const [stories, universeBooks, universeChapters] = await Promise.all([
        this.gateway.listStories(universeId),
        this.gateway.listBooksByUniverse(universeId),
        this.gateway.listChaptersByUniverse(universeId),
      ]);
      if (revision !== this.loadRevision || this.requestedUniverseId !== universeId) return;
      this.stories.set(stories);
      this.universeBooks.set(universeBooks);
      this.universeChapters.set(universeChapters);
      this.expandedStoryIds.set(new Set(stories.slice(0, 1).map((story) => story.id)));
      if (stories.length) await this.selectStory(stories[0]);
      else this.clearSelection();
    } catch (error) {
      if (revision === this.loadRevision) this.setError(error, 'Não foi possível carregar histórias, livros e capítulos.');
    }
  }

  reset(): void {
    this.loadRevision += 1;
    this.requestedUniverseId = '';
    if (this.saveTimer) { clearTimeout(this.saveTimer); this.saveTimer = null; }
    this.stories.set([]);
    this.universeBooks.set([]);
    this.universeChapters.set([]);
    this.expandedStoryIds.set(new Set());
    this.expandedBookIds.set(new Set());
    this.clearSelection();
  }

  clearError(): void {
    this.error.set('');
  }

  toggleInspector(): void {
    this.inspectorOpen.update((open) => !open);
    localStorage.setItem('narrahub.inspectorOpen', String(this.inspectorOpen()));
  }

  private clearSelection(): void {
    this.activeStory.set(null);
    this.activeBook.set(null);
    this.activeChapter.set(null);
    this.books.set([]);
    this.chapters.set([]);
    this.editorTitle.set('');
    this.editorContent.set('');
    this.chapterSummary.set('');
    this.saveMessage.set('');
  }

  async createStory(universeId: string, name: string): Promise<Story | null> {
    const trimmed = name.trim();
    if (!universeId || !trimmed) return null;
    try {
      const story = await this.gateway.createStory(universeId, trimmed);
      this.stories.set(await this.gateway.listStories(universeId));
      this.setExpanded(this.expandedStoryIds, story.id, true);
      await this.selectStory(story);
      return story;
    } catch (error) {
      this.setError(error, 'Não foi possível criar a história.');
      return null;
    }
  }

  async selectStory(story: Story): Promise<void> {
    await this.saveNow();
    const universeId = this.requestedUniverseId;
    const books = await this.gateway.listBooksByStory(story.id);
    if (this.requestedUniverseId !== universeId || !this.stories().some((item) => item.id === story.id)) return;
    this.activeStory.set(story);
    this.books.set(books);
    this.setExpanded(this.expandedStoryIds, story.id, true);
    if (this.books().length) await this.selectBook(this.books()[0]);
    else {
      this.activeBook.set(null);
      this.activeChapter.set(null);
      this.chapters.set([]);
      this.editorTitle.set('');
      this.editorContent.set('');
      this.chapterSummary.set('');
    }
  }

  async createBook(storyId: string, name: string): Promise<Book | null> {
    const trimmed = name.trim();
    const story = this.activeStory();
    if (!story || story.id !== storyId || !trimmed) return null;
    try {
      const book = await this.gateway.createBook(story.id, trimmed);
      this.books.set(await this.gateway.listBooksByStory(story.id));
      await this.refreshUniverseBooks();
      this.setExpanded(this.expandedBookIds, book.id, true);
      await this.selectBook(book);
      return book;
    } catch (error) {
      this.setError(error, 'Não foi possível criar o livro.');
      return null;
    }
  }

  async selectBook(book: Book): Promise<void> {
    await this.saveNow();
    const storyId = this.activeStory()?.id;
    const chapters = await this.gateway.listChaptersByBook(book.id);
    if (!storyId || this.activeStory()?.id !== storyId || !this.books().some((item) => item.id === book.id)) return;
    this.activeBook.set(book);
    this.chapters.set(chapters);
    this.setExpanded(this.expandedBookIds, book.id, true);
    if (this.chapters().length) await this.selectChapter(this.chapters()[0]);
    else {
      this.activeChapter.set(null);
      this.editorTitle.set('');
      this.editorContent.set('');
      this.chapterSummary.set('');
    }
  }

  async createChapter(bookId: string, title: string): Promise<Chapter | null> {
    const trimmed = title.trim();
    const book = this.activeBook();
    if (!book || book.id !== bookId || !trimmed) return null;
    try {
      const chapter = await this.gateway.createChapter(book.id, trimmed);
      this.chapters.set(await this.gateway.listChaptersByBook(book.id));
      await this.selectChapter(chapter);
      await this.refreshUniverseChapters();
      return chapter;
    } catch (error) {
      this.setError(error, 'Não foi possível criar o capítulo.');
      return null;
    }
  }

  async selectChapter(chapter: Chapter): Promise<void> {
    if (this.activeChapter()?.id === chapter.id) return;
    await this.saveNow();
    this.activeChapter.set(chapter);
    this.editorTitle.set(chapter.title);
    this.editorContent.set(chapter.content || '');
    this.chapterSummary.set(chapter.summary || '');
    this.saveMessage.set('Salvo');
  }

  booksForStory(storyId: string): BookOption[] {
    return this.universeBooks().filter((book) => book.story_id === storyId);
  }

  chaptersForBook(bookId: string): ChapterOption[] {
    return this.universeChapters().filter((chapter) => chapter.book_id === bookId);
  }

  isStoryExpanded(id: string): boolean {
    return this.expandedStoryIds().has(id);
  }

  isBookExpanded(id: string): boolean {
    return this.expandedBookIds().has(id);
  }

  toggleStory(id: string): void {
    this.setExpanded(this.expandedStoryIds, id, !this.isStoryExpanded(id));
  }

  toggleBook(id: string): void {
    this.setExpanded(this.expandedBookIds, id, !this.isBookExpanded(id));
  }

  /**
   * Atualiza histórias/livros/capítulos depois de uma mudança externa (ex.:
   * revisão de colaboração aplicada em outra sessão) SEM resetar a seleção
   * atual como `load()` faz — só recarrega as listas e, se houver um capítulo
   * ativo, busca o conteúdo mais recente dele.
   */
  async refreshAfterExternalChange(universeId: string): Promise<void> {
    if (this.requestedUniverseId !== universeId) return;
    const [stories, universeBooks, universeChapters] = await Promise.all([
      this.gateway.listStories(universeId),
      this.gateway.listBooksByUniverse(universeId),
      this.gateway.listChaptersByUniverse(universeId),
    ]);
    if (this.requestedUniverseId !== universeId) return;
    this.stories.set(stories);
    this.universeBooks.set(universeBooks);
    this.universeChapters.set(universeChapters);
    const activeChapterId = this.activeChapter()?.id;
    if (activeChapterId) {
      const chapter = await this.gateway.getChapter(activeChapterId);
      if (chapter && this.activeChapter()?.id === activeChapterId) {
        this.activeChapter.set(chapter);
        this.chapters.update((items) => items.map((item) => item.id === chapter.id ? chapter : item));
        this.editorTitle.set(chapter.title);
        this.editorContent.set(chapter.content || '');
        this.chapterSummary.set(chapter.summary || '');
      }
    }
  }

  /** Snapshot independente da seleção ativa — usado pelo Compartilhamento (Colaboração) para exportar qualquer universo, não só o aberto no momento. */
  listChaptersSnapshot(universeId: string): Promise<ChapterOption[]> {
    return this.gateway.listChaptersByUniverse(universeId);
  }

  /**
   * Recarrega as listas agregadas do universo (usadas por busca global e pelo
   * seletor de capítulos do Planejamento) depois de uma mutação de outro
   * domínio que não altera história/livro/capítulo diretamente, mas cujo
   * comportamento anterior já disparava esse refresh defensivo (ex.: excluir
   * uma entidade).
   */
  async refreshUniverseLists(): Promise<void> {
    await Promise.all([this.refreshUniverseBooks(), this.refreshUniverseChapters()]);
  }

  /** Abre a história/livro/capítulo pelo ID mesmo que ainda não sejam a seleção ativa (busca global, planejamento). */
  async openStoryById(id: string): Promise<Story | null> {
    const story = this.stories().find((item) => item.id === id);
    if (story) await this.selectStory(story);
    return story ?? null;
  }

  async openBookOption(option: BookOption): Promise<boolean> {
    const story = this.stories().find((item) => item.id === option.story_id);
    if (!story) return false;
    await this.selectStory(story);
    const book = this.books().find((item) => item.id === option.id);
    if (book) await this.selectBook(book);
    return !!book;
  }

  async openChapterOption(option: ChapterOption): Promise<boolean> {
    const story = this.stories().find((item) => item.id === option.story_id);
    if (!story) return false;
    await this.selectStory(story);
    const book = this.books().find((item) => item.id === option.book_id);
    if (!book) return false;
    await this.selectBook(book);
    const chapter = this.chapters().find((item) => item.id === option.id);
    if (chapter) await this.selectChapter(chapter);
    return !!chapter;
  }

  async renameStory(id: string, name: string): Promise<boolean> {
    const trimmed = name.trim();
    if (!trimmed) return false;
    try {
      await this.gateway.updateStory(id, { name: trimmed });
      this.stories.update((items) => items.map((item) => item.id === id ? { ...item, name: trimmed } : item));
      this.activeStory.update((item) => item?.id === id ? { ...item, name: trimmed } : item);
      this.universeBooks.update((items) => items.map((item) => item.story_id === id ? { ...item, story_name: trimmed } : item));
      this.universeChapters.update((items) => items.map((item) => item.story_id === id ? { ...item, story_name: trimmed } : item));
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível renomear a história.');
      return false;
    }
  }

  async renameBook(id: string, name: string): Promise<boolean> {
    const trimmed = name.trim();
    if (!trimmed) return false;
    try {
      await this.gateway.updateBook(id, { name: trimmed });
      this.books.update((items) => items.map((item) => item.id === id ? { ...item, name: trimmed } : item));
      this.activeBook.update((item) => item?.id === id ? { ...item, name: trimmed } : item);
      this.universeBooks.update((items) => items.map((item) => item.id === id ? { ...item, name: trimmed } : item));
      this.universeChapters.update((items) => items.map((item) => item.book_id === id ? { ...item, book_name: trimmed } : item));
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível renomear o livro.');
      return false;
    }
  }

  async renameChapter(id: string, name: string): Promise<boolean> {
    const trimmed = name.trim();
    if (!trimmed) return false;
    try {
      await this.gateway.updateChapterTitle(id, trimmed);
      this.chapters.update((items) => items.map((item) => item.id === id ? { ...item, title: trimmed } : item));
      this.universeChapters.update((items) => items.map((item) => item.id === id ? { ...item, title: trimmed } : item));
      this.activeChapter.update((item) => item?.id === id ? { ...item, title: trimmed } : item);
      if (this.activeChapter()?.id === id) { this.editorTitle.set(trimmed); this.saveMessage.set('Salvo'); }
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível renomear o capítulo.');
      return false;
    }
  }

  async deleteStory(id: string): Promise<boolean> {
    const wasActive = this.activeStory()?.id === id;
    try {
      if (wasActive) this.clearSelection();
      await this.gateway.deleteStory(id);
      const universeId = this.requestedUniverseId;
      if (!universeId) return true;
      this.stories.set(await this.gateway.listStories(universeId));
      if (wasActive && this.stories().length) await this.selectStory(this.stories()[0]);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível excluir a história.');
      return false;
    }
  }

  async deleteBook(id: string): Promise<boolean> {
    const story = this.activeStory();
    const wasActive = this.activeBook()?.id === id;
    try {
      if (wasActive) {
        this.activeBook.set(null);
        this.activeChapter.set(null);
        this.chapters.set([]);
        this.editorTitle.set('');
        this.editorContent.set('');
      }
      await this.gateway.deleteBook(id);
      if (!story) return true;
      this.books.set(await this.gateway.listBooksByStory(story.id));
      if (wasActive && this.books().length) await this.selectBook(this.books()[0]);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível excluir o livro.');
      return false;
    }
  }

  async deleteChapter(id: string): Promise<boolean> {
    const book = this.activeBook();
    const wasActive = this.activeChapter()?.id === id;
    try {
      if (wasActive) {
        this.activeChapter.set(null);
        this.editorTitle.set('');
        this.editorContent.set('');
        this.saveMessage.set('');
      }
      await this.gateway.deleteChapter(id);
      if (!book) return true;
      this.chapters.set(await this.gateway.listChaptersByBook(book.id));
      if (wasActive && this.chapters().length) await this.selectChapter(this.chapters()[0]);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível excluir o capítulo.');
      return false;
    }
  }

  async reorderChaptersInBook(bookId: string, orderedChapterIds: string[]): Promise<void> {
    try {
      await this.gateway.reorderChapters(bookId, orderedChapterIds);
      await this.refreshUniverseChapters();
      if (this.activeBook()?.id === bookId) this.chapters.set(await this.gateway.listChaptersByBook(bookId));
    } catch (error) {
      this.setError(error, 'Não foi possível reordenar os capítulos.');
    }
  }

  async updateBookCover(bookId: string, coverImage: string): Promise<boolean> {
    try {
      await this.gateway.updateBook(bookId, { coverImage });
      this.books.update((items) => items.map((item) => item.id === bookId ? { ...item, cover_image: coverImage } : item));
      this.universeBooks.update((items) => items.map((item) => item.id === bookId ? { ...item, cover_image: coverImage } : item));
      this.activeBook.update((item) => item?.id === bookId ? { ...item, cover_image: coverImage } : item);
      return true;
    } catch (error) {
      this.setError(error, 'Não foi possível atualizar a capa do livro.');
      return false;
    }
  }

  async updateSceneRoute(sceneOrigin: string, sceneDestination: string): Promise<void> {
    const chapter = this.activeChapter();
    if (!chapter) return;
    try {
      await this.gateway.updateChapterSceneRoute(chapter.id, sceneOrigin, sceneDestination);
      const updated = { ...chapter, scene_origin: sceneOrigin, scene_destination: sceneDestination };
      this.activeChapter.set(updated);
      this.chapters.update((items) => items.map((item) => item.id === chapter.id ? updated : item));
      this.universeChapters.update((items) => items.map((item) => item.id === chapter.id ? { ...item, ...updated } : item));
    } catch (error) {
      this.setError(error, 'Não foi possível salvar a rota de cena.');
    }
  }

  setEditorTitle(value: string): void { this.editorTitle.set(value); this.queueSave(); }
  setEditorContent(value: string): void { this.editorContent.set(value); this.queueSave(); }
  setChapterSummary(value: string): void { this.chapterSummary.set(value); this.queueSave(); }

  private queueSave(): void {
    if (!this.activeChapter()) return;
    this.saveMessage.set('Alterações pendentes');
    if (this.saveTimer) clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => void this.saveNow(), 700);
  }

  async saveNow(): Promise<SavedChapter | null> {
    if (this.saveTimer) { clearTimeout(this.saveTimer); this.saveTimer = null; }
    const chapter = this.activeChapter();
    if (!chapter || this.saveMessage() === 'Salvo') return null;
    const title = this.editorTitle().trim() || 'Capítulo sem título';
    const content = this.editorContent();
    const summary = this.chapterSummary().trim();
    const words = this.countWords(content);
    const contentChanged = content !== chapter.content || words !== chapter.word_count;
    this.isSaving.set(true);
    try {
      if (title !== chapter.title) await this.gateway.updateChapterTitle(chapter.id, title);
      if (contentChanged) await this.gateway.updateChapterContent(chapter.id, content, words);
      if (summary !== (chapter.summary || '')) await this.gateway.updateChapterSummary(chapter.id, summary);
      const updated: Chapter = { ...chapter, title, content, summary, word_count: words };
      this.activeChapter.set(updated);
      this.chapters.update((items) => items.map((item) => item.id === updated.id ? updated : item));
      this.universeChapters.update((items) => items.map((item) => item.id === updated.id ? { ...item, ...updated } : item));
      this.saveMessage.set('Salvo');
      this.onChapterPersisted?.(updated.id, content);
      return { chapter: updated, contentChanged };
    } catch (error) {
      this.saveMessage.set('Erro ao salvar');
      this.setError(error, 'O capítulo não foi salvo.');
      return null;
    } finally {
      this.isSaving.set(false);
    }
  }

  private async refreshUniverseChapters(): Promise<void> {
    if (!this.requestedUniverseId) return;
    const chapters = await this.gateway.listChaptersByUniverse(this.requestedUniverseId);
    this.universeChapters.set(chapters);
  }

  private async refreshUniverseBooks(): Promise<void> {
    if (!this.requestedUniverseId) return;
    const books = await this.gateway.listBooksByUniverse(this.requestedUniverseId);
    this.universeBooks.set(books);
  }

  private setExpanded(target: typeof this.expandedStoryIds, id: string, expanded: boolean): void {
    target.update((current) => {
      const next = new Set(current);
      if (expanded) next.add(id); else next.delete(id);
      return next;
    });
  }

  private countWords(content: string): number {
    const normalized = content.replace(/<[^>]+>/g, ' ').trim();
    return normalized ? normalized.split(/\s+/u).length : 0;
  }

  private setError(error: unknown, fallback: string): void {
    console.error('[NarraHub] Manuscript operation failed', error);
    const message = error instanceof Error ? error.message : String(error || '');
    this.error.set(message && !/database|sqlite|constraint/iu.test(message) ? message : fallback);
  }
}
