import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { normalizeNativeCommandError } from '../errors/native-command-error';

/** O que o arranque fez com os assets. Espelha `ResumoDoArranque` no Rust. */
export interface StorageUpgradeSummary {
  haviaTrabalho: boolean;
  migrados: number;
  reconstruidos: number;
  inlineLimpo: number;
  pendenciasAbertas: number;
}

/**
 * A fronteira do blob store (ADR 0010).
 *
 * O documento persistido guarda `data-narrahub-blob="<sha256>"`, e nada mais.
 * Este serviço faz as duas travessias:
 *
 *     arquivo    →  bytes  →  publish()  →  hash        (escrita)
 *     hash       →  resolve()            →  URL local   (leitura, em runtime)
 *
 * **A URL que `resolve()` devolve nunca volta ao banco.** Ela é um
 * `blob:` da própria aba, montado em memória, e morre com ela. O que vai ao
 * documento é o hash — é o que faz o mesmo capítulo funcionar no Windows e no
 * Android, e é o que sobrevive a restaurar um backup noutra máquina.
 *
 * O base64 atravessa o IPC porque o IPC carrega texto. Isso é transporte: ele
 * existe entre o arquivo e o `blob_put`, e entre o `blob_read` e o objeto de
 * URL. Persistir base64 é o que esta etapa inteira existe para impedir.
 */
@Injectable({ providedIn: 'root' })
export class BlobService {
  /**
   * O cache é por hash, e isso é consequência do endereçamento por conteúdo:
   * o mesmo hash é sempre o mesmo arquivo, então a URL resolvida uma vez vale
   * para todas as ocorrências — a mesma imagem repetida cem vezes num capítulo
   * é uma leitura.
   */
  private readonly resolvidas = new Map<string, string>();

  /** 64 hexadecimais minúsculos, como o Rust exige. */
  static isCanonicalHash(hash: string): boolean {
    return /^[0-9a-f]{64}$/u.test(hash);
  }

  /** Publica os bytes de um arquivo e devolve o `SHA-256` deles. */
  async publish(file: File): Promise<string> {
    const bytes = new Uint8Array(await file.arrayBuffer());
    const base64 = toBase64(bytes);
    try {
      return await invoke<string>('blob_put', { base64 });
    } catch (error) {
      throw normalizeNativeCommandError(error);
    }
  }

  /**
   * Resolve um hash para algo que o `<img>` consegue mostrar.
   *
   * Devolve `null` quando o blob não está aqui — estado normal no
   * incremental, porque a referência pode chegar antes do arquivo. Quem chama
   * deixa o node sem `src` em vez de inventar um caminho.
   */
  async resolve(hash: string): Promise<string | null> {
    if (!BlobService.isCanonicalHash(hash)) return null;
    const jaResolvida = this.resolvidas.get(hash);
    if (jaResolvida) return jaResolvida;
    if (!isTauri()) return null;

    try {
      const lido = await invoke<{ hash: string; base64: string; sizeBytes: number }>('blob_read', {
        hash,
      });
      const bytes = fromBase64(lido.base64);
      const url = URL.createObjectURL(new Blob([bytes as BlobPart]));
      this.resolvidas.set(hash, url);
      return url;
    } catch {
      // Blob ausente ou corrompido. O Rust já recusou servir bytes que não
      // batem com o endereço pedido — mostrar a imagem errada seria pior que
      // não mostrar nenhuma.
      return null;
    }
  }

  /**
   * A fronteira de upgrade dos assets (ADR 0010).
   *
   * Chamada uma vez no arranque, entre o `Database.load` — que é onde as
   * migrations rodam — e o primeiro consumo do acervo. Converte o legado de
   * mídia: publica os blobs, grava as referências, limpa o inline, e registra
   * como pendência o que não soube converter.
   *
   * **Não bloqueia a abertura.** Pendência de mídia é problema de mídia; quem
   * exige o contrato completo é o bootstrap de pareamento, e ele já recusa.
   */
  async prepareAssets(): Promise<StorageUpgradeSummary | null> {
    if (!isTauri()) return null;
    try {
      return await invoke<StorageUpgradeSummary>('storage_prepare_assets');
    } catch (error) {
      throw normalizeNativeCommandError(error);
    }
  }

  /** Libera as URLs montadas. Chamado quando o editor é destruído. */
  releaseAll(): void {
    for (const url of this.resolvidas.values()) URL.revokeObjectURL(url);
    this.resolvidas.clear();
  }
}

function toBase64(bytes: Uint8Array): string {
  // Em blocos, porque `String.fromCharCode(...bytes)` estoura a pilha com
  // alguns megabytes de argumentos.
  let texto = '';
  const bloco = 0x8000;
  for (let inicio = 0; inicio < bytes.length; inicio += bloco) {
    texto += String.fromCharCode(...bytes.subarray(inicio, inicio + bloco));
  }
  return btoa(texto);
}

function fromBase64(base64: string): Uint8Array {
  const binario = atob(base64);
  const bytes = new Uint8Array(binario.length);
  for (let indice = 0; indice < binario.length; indice += 1) {
    bytes[indice] = binario.charCodeAt(indice);
  }
  return bytes;
}
