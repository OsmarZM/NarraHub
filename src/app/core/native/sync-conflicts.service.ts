import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { normalizeNativeCommandError } from '../errors/native-command-error';

/**
 * A porta dos conflitos do Sync V2 (etapa F).
 *
 * O backend entrega os conflitos já interpretados (`application::conflitos` no Rust): títulos
 * legíveis, as duas versões campo a campo, diff de texto e as ações possíveis. **Este serviço não
 * conhece tabela de sincronização nem envelope** — e o teste de fronteira cobra isso.
 *
 * "Deste aparelho" e "do outro aparelho" são só apresentação. A ação que volta é portátil
 * (`ficarComA`, `restaurar`, …): dois aparelhos que escolhem a mesma versão escolhem a mesma coisa.
 */

/** O filtro da lista. Campo vazio = sem filtro. */
export interface ConflictFilter {
  universeId: string;
  aggregateType: string;
  kind: string;
  /** `aberto`, `resolvido` ou vazio. */
  status: string;
}

export interface ConflictSummary {
  conflictKey: string;
  kind: string;
  descricao: string;
  aggregateType: string;
  aggregateId: string;
  rotuloDoTipo: string;
  titulo: string;
  universeId: string;
  status: 'aberto' | 'resolvido';
  detectadoEm: string;
  resolvidoEm: string;
  acaoInteira: boolean;
}

export interface ConflictField {
  campo: string;
  rotulo: string;
  valor: unknown;
}

export interface ConflictSide {
  letra: 'a' | 'b';
  desteAparelho: boolean;
  excluido: boolean;
  titulo: string;
  campos: ConflictField[];
  indisponivel: boolean;
}

export interface ConflictDifference {
  campo: string;
  rotulo: string;
  desteAparelho: unknown;
  doOutro: unknown;
}

export interface ConflictDiffLine {
  tipo: 'igual' | 'so_deste' | 'so_do_outro';
  texto: string;
}

export interface ConflictAction {
  acao: ConflictActionType;
  rotulo: string;
  descricao: string;
  pedeNome: boolean;
  tagId: string;
}

export interface ConflictDetail {
  resumo: ConflictSummary;
  desteAparelho: ConflictSide;
  doOutro: ConflictSide;
  diferencas: ConflictDifference[];
  diffDeTexto: ConflictDiffLine[];
  acoes: ConflictAction[];
}

export type ConflictActionType =
  | 'ficarComA'
  | 'ficarComB'
  | 'restaurar'
  | 'manterExclusao'
  | 'manterLocal'
  | 'aceitarExclusao'
  | 'renomear'
  | 'mesclar';

/** O que o comando de resolver recebe: a ação portátil, e o nome quando ela pede. */
export interface ConflictResolutionRequest {
  tipo: ConflictActionType;
  tagId?: string;
  nome?: string;
}

export interface ConflictResolutionResult {
  conflictKey: string;
  resolutionRev: string;
}

/** O aviso da atualização que girou a época causal (E0-beta). */
export interface EpochNotice {
  pareamentosInvalidados: boolean;
  divergenciasArquivadas: number;
  iniciadaEm: string;
}

@Injectable({ providedIn: 'root' })
export class SyncConflictsService {
  async list(filter: Partial<ConflictFilter> = {}): Promise<ConflictSummary[]> {
    if (!isTauri()) return [];
    return this.call<ConflictSummary[]>(
      'sync_conflitos_listar',
      { filtro: { universeId: '', aggregateType: '', kind: '', status: '', ...filter } },
      'Os conflitos não puderam ser lidos.',
    );
  }

  async inspect(conflictKey: string): Promise<ConflictDetail> {
    return this.call<ConflictDetail>(
      'sync_conflito_inspecionar',
      { conflictKey },
      'O conflito não pôde ser aberto.',
    );
  }

  async resolve(conflictKey: string, acao: ConflictResolutionRequest): Promise<ConflictResolutionResult> {
    return this.call<ConflictResolutionResult>(
      'sync_conflito_resolver',
      { conflictKey, acao },
      'A decisão não pôde ser registrada.',
    );
  }

  async epochNotice(): Promise<EpochNotice | null> {
    if (!isTauri()) return null;
    return this.call<EpochNotice>('sync_v2_aviso_de_epoca', {}, 'O aviso da sincronização não pôde ser lido.');
  }

  private async call<T>(command: string, args: Record<string, unknown>, fallback: string): Promise<T> {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      throw normalizeNativeCommandError(error, fallback);
    }
  }
}
