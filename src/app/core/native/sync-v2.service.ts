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

/**
 * A porta do Sync V2 no frontend (etapa 14, fatia 2).
 *
 * **Não é o `SyncService`.** Aquele fala com o Sync V1 — `sync_start`,
 * `sync_connect` —, que copia tabelas inteiras sem passar pelo log de eventos.
 * A decisão registrada é congelar e substituir, sem coexistir: este serviço
 * não chama nada do V1, e o V1 não recebe nada novo.
 *
 * `panorama()` é só leitura. Escuta e pareamento entram na fatia 3, junto com
 * o transporte que dá a eles algo para conversar.
 */
@Injectable({ providedIn: 'root' })
export class SyncV2Service {
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
