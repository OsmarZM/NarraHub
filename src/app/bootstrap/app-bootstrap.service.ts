import { Injectable, inject, signal } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';
import { SyncSessionFeedbackService } from '../application/sync-session-feedback.service';
import { AiService } from '../core/native/ai.service';
import { BackupService } from '../core/native/backup.service';
import { BlobService } from '../core/native/blob.service';
import { DatabaseService } from '../core/services/database.service';
import { DatabaseCompatibility } from './database-compatibility';
import { CollaborationStore } from '../features/collaboration/state/collaboration.store';
import { KnowledgeStore } from '../features/knowledge/state/knowledge.store';
import { UniverseStore } from '../features/library/state/universe.store';
import { SettingsStore } from '../features/settings/state/settings.store';

@Injectable({ providedIn: 'root' })
export class AppBootstrapService {
  private readonly ai = inject(AiService);
  private readonly db = inject(DatabaseService);
  private readonly backupService = inject(BackupService);
  private readonly blobs = inject(BlobService);
  private readonly collaboration = inject(CollaborationStore);
  private readonly knowledge = inject(KnowledgeStore);
  private readonly universes = inject(UniverseStore);
  private readonly settings = inject(SettingsStore);
  private readonly syncFeedback = inject(SyncSessionFeedbackService);

  readonly ready = signal(false);
  readonly error = signal('');
  /**
   * Preenchido quando o banco no disco é mais novo que este executável. Nesse caso o pool
   * **não** é aberto e a interface mostra a tela de recuperação. Ver ADR 0007.
   */
  readonly schemaIncompatible = signal<DatabaseCompatibility | null>(null);

  private initialization: Promise<void> | null = null;
  private collaborationTimer: ReturnType<typeof setInterval> | null = null;
  private updateTimer: ReturnType<typeof setTimeout> | null = null;

  initialize(): Promise<void> {
    this.initialization ??= this.runInitialization();
    return this.initialization;
  }

  shutdown(): void {
    if (this.collaborationTimer) clearInterval(this.collaborationTimer);
    if (this.updateTimer) clearTimeout(this.updateTimer);
    this.collaborationTimer = null;
    this.updateTimer = null;
    this.settings.dispose();
    this.syncFeedback.stop();
    this.ai.dispose();
  }

  private async runInitialization(): Promise<void> {
    this.error.set('');
    try {
      await this.ai.initialize().catch((error) => {
        console.error('[NarraHub] Não foi possível inicializar o gerenciador da IA local.', error);
      });
      if (!isTauri()) return;

      // Portão do ADR 0007: perguntar qual schema está no disco antes de abrir o pool.
      // Um banco mais novo que este executável não pode ser aberto — abrir significaria
      // escrever em colunas que ele não conhece. E, sem esta verificação, a falha acontece
      // dentro do initializer, antes de a interface existir: o app morre sem dizer nada.
      const compatibility = await this.backupService.compatibility();
      if (!compatibility.compatible) {
        this.schemaIncompatible.set(compatibility);
        return;
      }

      await this.openDatabaseSafely();

      // O PREPARO DO ACERVO (ADR 0010 + NH-079 etapa C).
      //
      //   db.init()          o plugin-sql aplica as migrations
      //   prepareArchive()   ◄── aqui: mídia convertida, acervo adotado, banco liberado
      //   universes.load()   primeiro consumo do acervo
      //
      // A ordem dentro dessa chamada é do Rust, não daqui: conversão de mídia,
      // depois adoção, e só então o banco sai de "preparando". Nenhum comando
      // de domínio — inclusive os de sincronização — responde antes disso.
      //
      // **Erro aqui interrompe o arranque de propósito.** Até a etapa C a
      // falha era registrada e seguia adiante, porque só havia mídia em jogo;
      // agora, uma adoção que falha significa acervo sem passado causal, e
      // seguir abriria o aplicativo sobre um estado que a sincronização não
      // sabe descrever. O banco fica preservado, em recuperação, e o erro
      // aparece na tela.
      //
      // Pendência de mídia continua não travando nada: o texto abre, e o que
      // espera é a sincronização — com o motivo dito.
      const acervo = await this.blobs.prepareArchive();
      if (acervo?.assets?.haviaTrabalho) {
        console.log(
          `[NarraHub] Assets migrados: ${acervo.assets.migrados} publicados, `
            + `${acervo.assets.inlineLimpo} liberados do banco, `
            + `${acervo.assets.pendenciasAbertas} pendência(s).`,
        );
      }
      if (acervo && acervo.adocao.adotados > 0) {
        console.log(
          `[NarraHub] Acervo adotado pela sincronização: ${acervo.adocao.adotados} itens.`,
        );
      }
      if (acervo?.pareamentosInvalidados) {
        console.warn(
          '[NarraHub] A sincronização foi atualizada. Por segurança, pareie seus aparelhos novamente.',
        );
      }
      if (acervo && !acervo.sincronizacaoDisponivel) {
        console.warn(
          '[NarraHub] Sincronização indisponível nesta sessão: '
            + acervo.motivoDaIndisponibilidade,
        );
      }

      await this.universes.load();
      await this.knowledge.refreshLibraryPreviewTags();
      // Antes de qualquer escuta: uma sessão atendida precisa encontrar alguém ouvindo (I-BUG-03).
      await this.syncFeedback.start();
      await this.collaboration.refreshShareStatus();
      await this.collaboration.loadReview();
      this.collaborationTimer = setInterval(() => void this.collaboration.syncIncoming(), 2500);
      await this.settings.primeCurrentVersion();
      if (await this.settings.isUpdateConfigured()) {
        this.updateTimer = setTimeout(() => void this.settings.checkForUpdates(true), 1800);
      }
    } catch (error) {
      console.error('[NarraHub] Não foi possível inicializar a aplicação.', error);
      this.error.set(error instanceof Error ? error.message : String(error));
    } finally {
      this.ready.set(true);
    }
  }

  /**
   * Abre o banco com a migration protegida (ver `src-tauri/src/database/upgrade.rs`).
   *
   *   prepareMigration   backup validado antes; migration interrompida antes é desfeita
   *   db.init            o plugin-sql aplica as migrations
   *   finishMigration    só aqui o registro some; versão intermediária não passa
   *   rollbackMigration  qualquer falha acima devolve o banco original
   */
  private async openDatabaseSafely(): Promise<void> {
    const preparation = await this.backupService.prepareMigration();
    if (preparation.recoveredInterrupted) {
      console.warn('[NarraHub] Uma atualização do banco tinha sido interrompida; o banco anterior foi restaurado antes de tentar de novo.');
    }
    if (!preparation.needed) {
      await this.db.init();
      return;
    }
    console.log(
      `[NarraHub] Atualizando o banco da versão ${preparation.fromVersion} para a ${preparation.toVersion}. `
        + `Backup antes da atualização: ${preparation.backup?.backupId}.`,
    );
    try {
      await this.db.init();
      await this.backupService.finishMigration();
    } catch (error) {
      await this.db.close().catch(() => undefined);
      const detalhe = error instanceof Error ? error.message : String(error);
      try {
        const rollback = await this.backupService.rollbackMigration();
        throw new Error(
          `A atualização do banco falhou (${detalhe}). Seu banco anterior foi restaurado`
            + `${rollback.backupId ? ` a partir do backup ${rollback.backupId}` : ''} e nada foi perdido.`,
        );
      } catch (rollbackError) {
        if (rollbackError instanceof Error && rollbackError.message.startsWith('A atualização do banco falhou')) throw rollbackError;
        const motivo = rollbackError instanceof Error ? rollbackError.message : String(rollbackError);
        throw new Error(
          `A atualização do banco falhou (${detalhe}) e o banco anterior não pôde ser restaurado automaticamente (${motivo}). `
            + `O backup ${preparation.backup?.backupId ?? ''} continua em Configurações → Backup.`,
        );
      }
    }
  }
}
