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

/** O que a adoção do acervo fez (NH-079 etapa C). */
export interface ArchiveAdoptionSummary {
  jaEstavaAdotado: boolean;
  adotados: number;
  eventos: number;
  primeiroSeq: number;
  ultimoSeq: number;
}

/** O resultado do preparo do acervo no arranque: mídia, adoção e o veredito do sync. */
export interface ArchivePreparation {
  assets: StorageUpgradeSummary;
  adocao: ArchiveAdoptionSummary;
  sincronizacaoDisponivel: boolean;
  motivoDaIndisponibilidade: string;
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
      throw normalizeNativeCommandError(error, 'A imagem não pôde ser guardada neste aparelho.');
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
   * O preparo do acervo no arranque (ADR 0010 + NH-079 etapa C).
   *
   * Uma chamada só, porque a ordem é obrigatória e o Rust é quem a garante:
   *
   *     conversão de mídia  →  adoção do acervo  →  Ready  →  sync disponível
   *
   * Chamada entre o `Database.load` — onde as migrations rodam — e o primeiro
   * consumo do acervo.
   *
   * **Pendência de mídia não bloqueia a abertura**: o texto continua
   * acessível e o que fica indisponível é a sincronização, com motivo em
   * `motivoDaIndisponibilidade`. **Erro, sim, bloqueia**: o banco fica em
   * recuperação, e este método propaga a causa em vez de engoli-la.
   */
  async prepareArchive(): Promise<ArchivePreparation | null> {
    if (!isTauri()) return null;
    try {
      return await invoke<ArchivePreparation>('storage_prepare_archive');
    } catch (error) {
      throw normalizeNativeCommandError(error, 'O acervo não pôde ser preparado para abrir.');
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
