import { Injectable, inject, signal } from '@angular/core';
import {
  LegacyRecoveryItem,
  LegacyRecoveryService,
  RecoveryDestination,
} from '../../../core/native/legacy-recovery.service';

/**
 * O estado da caixa de versões antigas (etapa H, H-R3).
 *
 * Sem gateway, como o `SettingsStore`: o domínio fala com comandos nativos, e não há fronteira
 * SQL-vs-Rust a abstrair aqui.
 */
@Injectable({ providedIn: 'root' })
export class LegacyRecoveryStore {
  private readonly service = inject(LegacyRecoveryService);

  readonly items = signal<LegacyRecoveryItem[]>([]);
  readonly destinations = signal<RecoveryDestination[]>([]);
  readonly selected = signal<LegacyRecoveryItem | null>(null);
  readonly loading = signal(false);
  readonly busy = signal(false);
  readonly error = signal('');
  readonly info = signal('');

  /** Quantas ainda esperam decisão. É o número do aviso em Configurações. */
  readonly pending = signal(0);

  async load(): Promise<void> {
    this.loading.set(true);
    this.error.set('');
    try {
      this.items.set(await this.service.list());
      this.pending.set(this.items().filter((item) => item.status === 'pending').length);
    } catch (error) {
      this.error.set(this.message(error, 'As versões antigas não puderam ser lidas.'));
    } finally {
      this.loading.set(false);
    }
  }

  /** Só o número — o aviso não depende de abrir a tela. */
  async refreshPending(): Promise<void> {
    try {
      this.pending.set(await this.service.pending());
    } catch {
      // A contagem é auxiliar: a tela mostra o erro quando for aberta.
    }
  }

  async open(item: LegacyRecoveryItem): Promise<void> {
    this.selected.set(item);
    this.info.set('');
    this.error.set('');
    try {
      this.destinations.set(await this.service.destinations(item.id));
    } catch (error) {
      this.destinations.set([]);
      this.error.set(this.message(error, 'Os livros não puderam ser lidos.'));
    }
  }

  close(): void {
    this.selected.set(null);
    this.destinations.set([]);
  }

  /** Preserva a versão antiga como capítulo novo. */
  async preserve(bookId: string, titulo: string): Promise<boolean> {
    const item = this.selected();
    if (!item || this.busy()) return false;
    if (!bookId) {
      this.error.set('Escolha o livro que vai receber o capítulo recuperado.');
      return false;
    }
    if (!titulo.trim()) {
      this.error.set('O capítulo recuperado precisa de um título.');
      return false;
    }
    this.busy.set(true);
    this.error.set('');
    try {
      await this.service.preserve({ id: item.id, bookId, titulo: titulo.trim() });
      this.info.set('Versão antiga preservada como capítulo novo. Ela sincroniza como qualquer conteúdo.');
      this.close();
      await this.load();
      return true;
    } catch (error) {
      this.error.set(this.message(error, 'A versão antiga não pôde ser preservada.'));
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  /** Descarta a pendência. A tela confirma antes de chamar. */
  async discard(): Promise<boolean> {
    const item = this.selected();
    if (!item || this.busy()) return false;
    this.busy.set(true);
    this.error.set('');
    try {
      await this.service.discard(item.id);
      this.info.set('Versão antiga descartada. O registro histórico continua guardado no banco.');
      this.close();
      await this.load();
      return true;
    } catch (error) {
      this.error.set(this.message(error, 'A versão antiga não pôde ser descartada.'));
      return false;
    } finally {
      this.busy.set(false);
    }
  }

  private message(error: unknown, fallback: string): string {
    if (error && typeof error === 'object' && 'message' in error) {
      const texto = String((error as { message: unknown }).message || '');
      if (texto) return texto;
    }
    return fallback;
  }
}
