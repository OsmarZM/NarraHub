import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';

/**
 * A porta da caixa de recuperação do legado (etapa H, H-R3).
 *
 * É plataforma, como as outras de `core/native`: o que ela lê é sobre o passado DESTE aparelho —
 * versões antigas que ficaram guardadas aqui quando a sincronização anterior foi desligada. A tela
 * recebe itens prontos; ela nunca vê a tabela de origem, e preservar passa pela mesma `Mutacao` de
 * qualquer conteúdo, virando capítulo normal que sincroniza pelo Sync V2.
 */
export interface LegacyRecoveryItem {
  id: string;
  /** `pending`, `preserved` ou `discarded`. */
  status: string;
  aggregateType: string;
  aggregateId: string;
  field: string;
  /** Título do capítulo original, quando ele ainda existe aqui. */
  tituloAtual: string;
  /** Livro do capítulo original, quando ele ainda existe aqui. */
  livroAtual: string;
  /** O que está no acervo hoje. Vazio quando o capítulo não existe mais. */
  versaoAtual: string;
  /** A versão antiga que só existe neste aparelho. */
  versaoAntiga: string;
  registradoEm: string;
  resolvidoEm: string;
  capituloPreservado: string;
}

export interface RecoveryDestination {
  bookId: string;
  bookName: string;
  storyName: string;
  universeId: string;
  universeName: string;
  /** É o livro do capítulo original: a tela pré-seleciona, e o escritor confirma. */
  eOLivroOriginal: boolean;
}

export interface PreserveRequest {
  id: string;
  bookId: string;
  titulo: string;
}

@Injectable({ providedIn: 'root' })
export class LegacyRecoveryService {
  /** Quantas versões antigas ainda esperam decisão. */
  async pending(): Promise<number> {
    if (!isTauri()) return 0;
    return this.call<number>('legado_pendentes', {}, 'As versões antigas não puderam ser contadas.');
  }

  async list(): Promise<LegacyRecoveryItem[]> {
    if (!isTauri()) return [];
    return this.call<LegacyRecoveryItem[]>('legado_listar', {}, 'As versões antigas não puderam ser lidas.');
  }

  async destinations(item: string): Promise<RecoveryDestination[]> {
    if (!isTauri()) return [];
    return this.call<RecoveryDestination[]>('legado_destinos', { item }, 'Os livros não puderam ser lidos.');
  }

  /** Cria um capítulo novo com a versão antiga. Devolve o id do capítulo criado. */
  async preserve(pedido: PreserveRequest): Promise<string> {
    return this.call<string>('legado_preservar', { pedido }, 'A versão antiga não pôde ser preservada.');
  }

  async discard(item: string): Promise<void> {
    await this.call<void>('legado_descartar', { item }, 'A versão antiga não pôde ser descartada.');
  }

  private async call<T>(comando: string, argumentos: Record<string, unknown>, fallback: string): Promise<T> {
    try {
      return await invoke<T>(comando, argumentos);
    } catch (error) {
      throw new Error(this.mensagem(error, fallback));
    }
  }

  private mensagem(error: unknown, fallback: string): string {
    if (typeof error === 'string' && error.trim()) return error;
    if (error && typeof error === 'object' && 'message' in error) {
      const texto = String((error as { message: unknown }).message || '');
      if (texto) return texto;
    }
    return fallback;
  }
}
