import { Injectable, inject } from '@angular/core';
import { SyncServedSession, SyncSessionResult, SyncV2Service } from '../core/native/sync-v2.service';
import { ConflictsStore } from '../features/conflicts/state/conflicts.store';
import { SettingsStore } from '../features/settings/state/settings.store';
import { ShellState } from '../shell/state/shell.state';
import { WorkspaceSyncService } from './workspace-sync.service';

/** O resumo de uma sessão, do ponto de vista deste aparelho. */
export function describeSyncSession(r: SyncSessionResult): string {
  const papel = r.papel === 'receptor' ? 'Acervo recebido de' : r.papel === 'doador' ? 'Acervo enviado para' : 'Sincronizado com';
  const contar = (n: number, um: string, varios: string) => `${n} ${n === 1 ? um : varios}`;
  return `${papel} ${r.parceiro.nome}: `
    + `${contar(r.eventosAplicados, 'alteração recebida', 'alterações recebidas')}, `
    + `${contar(r.eventosEnviados, 'alteração enviada', 'alterações enviadas')}, `
    + `${contar(r.blobsRecebidos, 'imagem recebida', 'imagens recebidas')}.`
    + (r.eventosPendentes ? ` ${contar(r.eventosPendentes, 'alteração aguarda', 'alterações aguardam')} a próxima sessão.` : '');
}

/**
 * O que a tela faz quando uma sessão de sincronização termina, dos dois lados (I-BUG-03).
 *
 * Quem iniciou a sessão recebe o resultado do comando; quem escutava recebe um evento do Rust.
 * Nos dois casos a sessão gravou no banco por fora dos stores, então o aviso e a releitura são
 * os mesmos — e vivem aqui, acima das features, e não na página de Configurações, que nem precisa
 * estar aberta no aparelho que escuta.
 */
@Injectable({ providedIn: 'root' })
export class SyncSessionFeedbackService {
  private readonly syncV2 = inject(SyncV2Service);
  private readonly shell = inject(ShellState);
  private readonly workspaceSync = inject(WorkspaceSyncService);
  private readonly conflicts = inject(ConflictsStore);
  private readonly settings = inject(SettingsStore);

  private unlisten: (() => void) | null = null;

  async start(): Promise<void> {
    if (this.unlisten) return;
    this.unlisten = await this.syncV2.onSessionServed((session) => void this.served(session));
  }

  stop(): void {
    this.unlisten?.();
    this.unlisten = null;
  }

  /** Uma sessão terminou com sucesso: dizer o que aconteceu e reler o que ela pode ter mudado. */
  async applied(result: SyncSessionResult): Promise<void> {
    this.shell.showInfo(describeSyncSession(result));
    await Promise.all([this.workspaceSync.onSyncSessionApplied(), this.conflicts.refreshOpenCount()]);
  }

  private async served(session: SyncServedSession): Promise<void> {
    // O PIN usado deixou de valer e o último resultado mudou: a tela da escuta relê o estado.
    void this.settings.refreshSyncStatus();
    if (session.resultado) {
      await this.applied(session.resultado);
      return;
    }
    if (session.erro) this.shell.showError(`Uma sincronização recebida não foi concluída: ${session.erro}`);
    await this.conflicts.refreshOpenCount();
  }
}
