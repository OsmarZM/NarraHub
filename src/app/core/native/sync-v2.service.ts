import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { normalizeNativeCommandError } from '../errors/native-command-error';

/**
 * Um aparelho do roster do Sync V2.
 *
 * `estado` vem do `CHECK` da tabela `sync_devices`, não de uma lista que o
 * frontend mantenha em paralelo.
 */
export interface KnownDevice {
  deviceId: string;
  nome: string;
  estado: 'active' | 'retired' | 'revoked';
  eEsteAparelho: boolean;
  introduzidoPor: string;
  baseline: number;
  ultimoAplicado: number;
}

/** O que ainda não foi aplicado, por origem. */
export interface OriginPending {
  deviceId: string;
  nome: string;
  eventos: number;
  primeiraSequencia: number;
}

/** O estado do Sync V2 neste aparelho. */
export interface SyncV2Panorama {
  deviceId: string;
  chavePublica: string;
  aparelhos: KnownDevice[];
  eventosDesteAparelho: number;
  eventosPendentes: number;
  pendentesPorOrigem: OriginPending[];
  conflitosAbertos: number;
  pendenciasDeMidia: number;
}

/** Quem estava do outro lado de uma sessão. O id é derivado da chave, nunca digitado. */
export interface SyncPartner {
  deviceId: string;
  nome: string;
}

/** O resultado do pareamento por QR (NH-084): a sessão e o endereço que o Rust validou. */
export interface SyncPairedByQr {
  resultado: SyncSessionResult;
  endereco: string;
}

/** O papel deste aparelho na sessão, decidido pelos dois lados juntos. */
export type SyncRole = 'doador' | 'receptor' | 'par';

/** O que uma sessão fez. */
export interface SyncSessionResult {
  parceiro: SyncPartner;
  papel: SyncRole;
  houveBootstrap: boolean;
  blobsRecebidos: number;
  eventosEnviados: number;
  eventosAplicados: number;
  eventosPendentes: number;
}

/** O estado da escuta deste aparelho. */
export interface SyncV2ListenState {
  escutando: boolean;
  porta: number | null;
  /** Endereços para digitar no outro aparelho. Sem descoberta automática nesta etapa. */
  enderecos: string[];
  /** Oito dígitos com espaço no meio, ou null quando não há código aberto. */
  pin: string | null;
  ultimoResultado: SyncSessionResult | null;
  ultimoErro: string | null;
}

/**
 * Uma sessão que a escuta DESTE aparelho atendeu. Chega por evento, não por resposta de comando:
 * quem escuta não pediu nada, e sem o aviso a tela seguiria mostrando o acervo de antes (I-BUG-03).
 */
export interface SyncServedSession {
  resultado: SyncSessionResult | null;
  erro: string | null;
}

/** O evento que o Rust emite ao fim de cada sessão atendida. */
export const SYNC_V2_SERVED_EVENT = 'sync-v2-sessao-atendida';

/** A porta padrão da escuta. O usuário pode trocar. */
export const SYNC_V2_DEFAULT_PORT = 45870;

/**
 * A porta do Sync V2 no frontend (etapas 14, fatias 2 e 4).
 *
 * É a única porta de sincronização do app: o protocolo antigo, que copiava tabelas
 * inteiras sem passar pelo log de eventos, saiu do runtime na etapa G.
 *
 * `panorama()` é só leitura. Escuta e pareamento entram na fatia 3, junto com
 * o transporte que dá a eles algo para conversar.
 */
@Injectable({ providedIn: 'root' })
export class SyncV2Service {
  async listenState(): Promise<SyncV2ListenState | null> {
    if (!isTauri()) return null;
    return this.call<SyncV2ListenState>('sync_v2_estado', {}, 'O estado da escuta não pôde ser lido.');
  }

  /** Abre a escuta e emite um PIN novo. */
  async startListening(port: number, deviceName: string): Promise<SyncV2ListenState> {
    return this.call<SyncV2ListenState>(
      'sync_v2_escuta_iniciar',
      { porta: port, nome: deviceName },
      'A escuta não pôde ser aberta neste aparelho.',
    );
  }

  async stopListening(): Promise<SyncV2ListenState> {
    return this.call<SyncV2ListenState>('sync_v2_escuta_parar', {}, 'A escuta não pôde ser encerrada.');
  }

  /** Troca o PIN aberto por um novo — o anterior deixa de valer. */
  async newPin(): Promise<SyncV2ListenState> {
    return this.call<SyncV2ListenState>('sync_v2_pin_novo', {}, 'Não foi possível gerar um código novo.');
  }

  /** Pareia com o aparelho que mostra o PIN. Pode terminar em bootstrap. */
  async pair(address: string, pin: string, deviceName: string): Promise<SyncSessionResult> {
    return this.call<SyncSessionResult>(
      'sync_v2_parear',
      { endereco: address, pin, nome: deviceName },
      'O pareamento não pôde ser concluído.',
    );
  }

  /**
   * O conteúdo do QR da escuta aberta (NH-084, PR B): texto opaco para virar imagem, ou `null` quando
   * não há código válido. O formato é do Rust; a tela não o interpreta.
   */
  async qrContent(): Promise<string | null> {
    if (!isTauri()) return null;
    return this.call<string | null>('sync_v2_qr', {}, 'O QR de pareamento não pôde ser gerado.');
  }

  /**
   * Pareia pelo texto cru que o leitor de QR devolveu. Quem interpreta e valida é o Rust, que devolve
   * também o endereço que ele leu — para "Sincronizar pareado" funcionar depois. Nunca o PIN.
   */
  async pairByQr(conteudo: string, deviceName: string): Promise<SyncPairedByQr> {
    return this.call<SyncPairedByQr>(
      'sync_v2_parear_por_qr',
      { conteudo, nome: deviceName },
      'O pareamento pelo QR não pôde ser concluído.',
    );
  }

  /** Sincroniza com um aparelho já pareado. */
  async syncWith(address: string, deviceName: string): Promise<SyncSessionResult> {
    return this.call<SyncSessionResult>(
      'sync_v2_sincronizar',
      { endereco: address, nome: deviceName },
      'A sincronização não pôde ser concluída.',
    );
  }

  /** Avisa a cada sessão que a escuta deste aparelho terminar. Devolve o cancelamento. */
  async onSessionServed(handler: (session: SyncServedSession) => void): Promise<() => void> {
    if (!isTauri()) return () => undefined;
    const { listen } = await import('@tauri-apps/api/event');
    return listen<SyncServedSession>(SYNC_V2_SERVED_EVENT, ({ payload }) => handler(payload));
  }

  private async call<T>(command: string, args: Record<string, unknown>, fallback: string): Promise<T> {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      throw normalizeNativeCommandError(error, fallback);
    }
  }

  async panorama(): Promise<SyncV2Panorama | null> {
    // Na prévia no navegador não existe motor nenhum. `null` diz "não há o que
    // mostrar", que é diferente de um panorama com tudo em zero — e a tela
    // precisa da diferença para não afirmar que o aparelho está sincronizado.
    if (!isTauri()) return null;
    try {
      return await invoke<SyncV2Panorama>('sync_v2_panorama');
    } catch (error) {
      throw normalizeNativeCommandError(
        error,
        'O estado da sincronização não pôde ser lido neste aparelho.',
      );
    }
  }
}
