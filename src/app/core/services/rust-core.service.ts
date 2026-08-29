import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';

/** Contrato de erro que os comandos do core devolvem (`DatabaseCommandError`). */
export type RustCoreErrorKind = 'validation' | 'not_found' | 'conflict' | 'storage' | 'unavailable';

export class RustCoreError extends Error {
  constructor(readonly kind: RustCoreErrorKind, message: string) {
    super(message);
    this.name = 'RustCoreError';
  }
}

/**
 * Porta única para os comandos do core Rust (Fase 4).
 *
 * Existe por um motivo só: `invoke()` rejeita com o objeto serializado pelo
 * Rust, não com um `Error`. Sem normalizar aqui, todo `catch` do frontend
 * receberia `{kind, message}` e o `error.message` sairia `undefined` na tela.
 */
@Injectable({ providedIn: 'root' })
export class RustCoreService {
  async call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      throw toRustCoreError(error, command);
    }
  }
}

function toRustCoreError(error: unknown, command: string): Error {
  if (error && typeof error === 'object' && 'kind' in error && 'message' in error) {
    const { kind, message } = error as { kind: RustCoreErrorKind; message: string };
    return new RustCoreError(kind, message);
  }
  if (error instanceof Error) return error;
  return new Error(`Falha no comando ${command}: ${String(error)}`);
}
