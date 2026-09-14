import { Injectable, inject, signal } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';
import { SyncResult, SyncServerStatus } from '../../../core/models';
import { BackupManifest, BackupService, BackupValidation, DatabaseHealthReport, RestorePreparation } from '../../../core/native/backup.service';
// DatabaseService is injected here on purpose, unlike the domain gateways: restoring a
// backup has to close and reopen the app's own SQLite connection pool, which is native
// pool lifecycle, not the SQL-vs-Rust boundary the other LegacyXGateway adapters abstract.
import { DatabaseService } from '../../../core/services/database.service';
import { SyncService } from '../../../core/native/sync.service';
import {
  SYNC_V2_DEFAULT_PORT,
  SyncSessionResult,
  SyncV2ListenState,
  SyncV2Service,
} from '../../../core/native/sync-v2.service';
import { AppUpdateInfo, UpdateService } from '../../../core/native/update.service';
import { AndroidUpdateService } from '../../../core/native/android-update.service';

export type UpdatePhase =
  | 'idle' | 'checking' | 'available' | 'backing-up' | 'downloading' | 'current' | 'error'
  // Só no Android: o APK foi baixado e conferido, e falta o usuário abrir o instalador.
  | 'ready'
  // Só no Android: o sistema pede que o NarraHub seja liberado para instalar apps.
  | 'permission';

export interface SettingsActionResult {
  ok: boolean;
  error?: string;
}

@Injectable({ providedIn: 'root' })
export class SettingsStore {
  private readonly backupService = inject(BackupService);
  private readonly updateService = inject(UpdateService);
  private readonly androidUpdate = inject(AndroidUpdateService);
  private readonly syncService = inject(SyncService);
  private readonly syncV2 = inject(SyncV2Service);
  private readonly db = inject(DatabaseService);

  readonly backupBusy = signal(false);
  readonly backupError = signal('');
  readonly databaseHealth = signal<DatabaseHealthReport | null>(null);
  readonly backups = signal<BackupManifest[]>([]);
  readonly lastBackupValidation = signal<BackupValidation | null>(null);
  readonly restorePreparation = signal<RestorePreparation | null>(null);

  readonly updateBusy = signal(false);
  readonly updateProgress = signal(0);
  readonly updatePhase = signal<UpdatePhase>('idle');
  readonly updateInfo = signal<AppUpdateInfo>({ currentVersion: '0.7.4', availableVersion: null, notes: '', publishedAt: null });
  readonly updateError = signal('');
  readonly updatePromptDismissed = signal(false);
  /**
   * `android` quando este aparelho atualiza por APK das GitHub Releases, `desktop` quando usa o
   * `tauri-plugin-updater`. Os dois passam pelos mesmos sinais de fase e progresso, então a tela de
   * Configurações e o aviso de inicialização servem aos dois.
   */
  readonly updateChannel = signal<'desktop' | 'android'>('desktop');

  readonly syncStatus = signal<SyncServerStatus>({ running: false, address: null, pairing_code: null, device_name: 'Meu computador' });
  readonly syncBusy = signal(false);

  // Sync V2 (etapa 14). O V1 acima continua no código só até o E2E físico
  // fechar; os dois nunca ficam ativos juntos — ver `syncV2Blocked`.
  readonly syncV2State = signal<SyncV2ListenState>({
    escutando: false,
    porta: null,
    enderecos: [],
    pin: null,
    ultimoResultado: null,
    ultimoErro: null,
  });
  readonly syncV2Busy = signal(false);
  readonly syncV2Port = SYNC_V2_DEFAULT_PORT;

  async refreshBackupStatus(): Promise<void> {
    if (!isTauri() || this.backupBusy()) return;
    this.backupBusy.set(true);
    this.backupError.set('');
    try {
      const [health, backups] = await Promise.all([this.backupService.health(), this.backupService.list()]);
      this.databaseHealth.set(health);
      this.backups.set(backups);
    } catch (error) {
      this.backupError.set(this.messageOf(error));
    } finally {
      this.backupBusy.set(false);
    }
  }

  async createBackup(reason: 'manual' | 'pre_update' = 'manual'): Promise<SettingsActionResult> {
    if (this.backupBusy()) return { ok: false };
    this.backupBusy.set(true);
    this.backupError.set('');
    this.lastBackupValidation.set(null);
    try {
      const manifest = await this.backupService.create(reason);
      const validation = await this.backupService.validate(manifest.backupId);
      this.lastBackupValidation.set(validation);
      this.backups.set(await this.backupService.list());
      this.databaseHealth.set(validation.databaseHealth);
      if (!validation.valid) throw new Error(validation.errors.join(' '));
      return { ok: true };
    } catch (error) {
      const message = this.messageOf(error);
      this.backupError.set(message);
      return { ok: false, error: message };
    } finally {
      this.backupBusy.set(false);
    }
  }

  async validateBackup(backupId: string): Promise<SettingsActionResult> {
    if (this.backupBusy()) return { ok: false };
    this.backupBusy.set(true);
    this.backupError.set('');
    try {
      const validation = await this.backupService.validate(backupId);
      this.lastBackupValidation.set(validation);
      if (!validation.valid) { const message = validation.errors.join(' '); this.backupError.set(message); return { ok: false, error: message }; }
      return { ok: true };
    } catch (error) {
      const message = this.messageOf(error);
      this.backupError.set(message);
      return { ok: false, error: message };
    } finally {
      this.backupBusy.set(false);
    }
  }

  async prepareRestore(backupId: string): Promise<SettingsActionResult> {
    if (this.backupBusy()) return { ok: false };
    this.backupBusy.set(true);
    this.backupError.set('');
    try {
      const preparation = await this.backupService.prepareRestore(backupId);
      this.restorePreparation.set(preparation);
      this.backups.set(await this.backupService.list());
      return { ok: true };
    } catch (error) {
      const message = this.messageOf(error);
      this.backupError.set(message);
      return { ok: false, error: message };
    } finally {
      this.backupBusy.set(false);
    }
  }

  async commitRestore(token: string): Promise<SettingsActionResult> {
    if (this.backupBusy()) return { ok: false };
    this.backupBusy.set(true);
    this.backupError.set('');
    try {
      await this.db.close();
      await this.backupService.commitRestore(token);
      await this.updateService.relaunch();
      return { ok: true };
    } catch (error) {
      await this.db.init().catch((reopenError) => console.error('[NarraHub] Database reopen failed after restore error.', reopenError));
      const message = this.messageOf(error);
      this.backupError.set(message);
      return { ok: false, error: message };
    } finally {
      this.backupBusy.set(false);
    }
  }

  clearRestorePreparation(): void {
    this.restorePreparation.set(null);
    this.backupError.set('');
  }

  backupReasonLabel(reason: BackupManifest['reason']): string {
    if (reason === 'manual') return 'Manual';
    if (reason === 'pre_update') return 'Antes de atualizar';
    if (reason === 'pre_migration') return 'Antes de migrar';
    if (reason === 'pre_restore') return 'Antes de restaurar';
    return 'Automático';
  }

  formatBackupSize(bytes: number): string {
    if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
    if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
    return `${Math.max(1, Math.round(bytes / 1_000))} KB`;
  }

  async primeCurrentVersion(): Promise<void> {
    const currentVersion = await this.updateService.currentVersion();
    this.updateInfo.update((info) => ({ ...info, currentVersion }));
    if (await this.androidUpdate.supported()) this.updateChannel.set('android');
  }

  isUpdateConfigured(): Promise<boolean> {
    return this.updateService.isConfigured();
  }

  async checkForUpdates(silent: boolean): Promise<{ ok: boolean; message: string }> {
    if (!isTauri()) return { ok: false, message: silent ? '' : 'A atualização automática funciona somente no aplicativo instalado.' };
    if (this.updateBusy()) return { ok: false, message: '' };
    if (this.updateChannel() === 'android' || (await this.androidUpdate.supported())) {
      this.updateChannel.set('android');
      return this.checkAndroidUpdate(silent);
    }
    if (!(await this.updateService.isConfigured())) {
      return { ok: false, message: silent ? '' : 'Este build de desenvolvimento não possui um canal de atualização configurado.' };
    }
    this.updateBusy.set(true);
    this.updatePhase.set('checking');
    this.updateError.set('');
    this.updateProgress.set(0);
    try {
      const info = await this.updateService.check();
      this.updateInfo.set(info);
      this.updatePhase.set(info.availableVersion ? 'available' : 'current');
      if (info.availableVersion) this.updatePromptDismissed.set(false);
      return { ok: true, message: info.availableVersion ? `Versão ${info.availableVersion} disponível.` : 'O NarraHub está atualizado.' };
    } catch (error) {
      this.updatePhase.set('error');
      this.updateError.set(this.messageOf(error));
      return { ok: false, message: '' };
    } finally {
      this.updateBusy.set(false);
    }
  }

  async installUpdate(): Promise<SettingsActionResult> {
    if (!this.updateInfo().availableVersion || this.updateBusy()) return { ok: false };
    if (this.updateChannel() === 'android') return this.downloadAndroidUpdate();
    this.updateBusy.set(true);
    this.updatePhase.set('backing-up');
    this.updateProgress.set(0);
    this.updateError.set('');
    this.backupBusy.set(true);
    try {
      const backup = await this.backupService.create('pre_update');
      const validation = await this.backupService.validate(backup.backupId);
      this.lastBackupValidation.set(validation);
      if (!validation.valid) throw new Error(`A atualização foi interrompida porque o backup de segurança não foi validado. ${validation.errors.join(' ')}`);
      this.backups.set(await this.backupService.list());
      this.databaseHealth.set(validation.databaseHealth);
      this.backupBusy.set(false);
      this.updatePhase.set('downloading');
      await this.updateService.downloadAndInstall((progress) => this.updateProgress.set(progress));
      await this.updateService.relaunch();
      return { ok: true };
    } catch (error) {
      const message = this.messageOf(error);
      this.updatePhase.set('error');
      this.updateError.set(message);
      return { ok: false, error: message };
    } finally {
      this.updateBusy.set(false);
      this.backupBusy.set(false);
    }
  }

  dismissUpdatePrompt(): void {
    this.updatePromptDismissed.set(true);
  }

  /**
   * "Depois": a novidade continua conhecida, mas sai da frente. Em Configurações o cartão volta ao
   * estado de verificar; o aviso de inicialização não reaparece até a próxima verificação.
   */
  postponeUpdate(): void {
    this.updatePromptDismissed.set(true);
    if (this.updatePhase() === 'available') this.updatePhase.set('idle');
  }

  // ── Android: APK assinado das GitHub Releases ─────────────

  private async checkAndroidUpdate(silent: boolean): Promise<{ ok: boolean; message: string }> {
    this.updateBusy.set(true);
    this.updatePhase.set('checking');
    this.updateError.set('');
    this.updateProgress.set(0);
    try {
      const result = await this.androidUpdate.check();
      const news = result.novidade;
      this.updateInfo.set({
        currentVersion: result.versaoAtual,
        availableVersion: news?.versao ?? null,
        notes: news ? news.notas || news.titulo : '',
        publishedAt: null,
      });
      this.updatePhase.set(news ? 'available' : 'current');
      if (news) this.updatePromptDismissed.set(false);
      return { ok: true, message: news ? `Versão ${news.versao} disponível.` : 'O NarraHub está atualizado.' };
    } catch (error) {
      this.updatePhase.set('error');
      this.updateError.set(this.messageOf(error));
      return { ok: false, message: silent ? '' : this.messageOf(error) };
    } finally {
      this.updateBusy.set(false);
    }
  }

  /**
   * Backup validado, depois o download conferido por SHA-256.
   *
   * A mesma regra do desktop: nenhuma versão nova é instalada sem um snapshot local válido. No
   * Android a instalação por cima preserva os dados do app, e o backup é a rede de segurança se
   * algo der errado fora do nosso controle.
   */
  private async downloadAndroidUpdate(): Promise<SettingsActionResult> {
    this.updateBusy.set(true);
    this.updatePhase.set('backing-up');
    this.updateProgress.set(0);
    this.updateError.set('');
    this.backupBusy.set(true);
    try {
      const backup = await this.backupService.create('pre_update');
      const validation = await this.backupService.validate(backup.backupId);
      this.lastBackupValidation.set(validation);
      if (!validation.valid) throw new Error(`A atualização foi interrompida porque o backup de segurança não foi validado. ${validation.errors.join(' ')}`);
      this.backupBusy.set(false);
      this.updatePhase.set('downloading');
      await this.androidUpdate.download((percent) => this.updateProgress.set(percent));
      this.updateProgress.set(100);
      this.updatePhase.set('ready');
      return { ok: true };
    } catch (error) {
      const message = this.messageOf(error);
      this.updatePhase.set('error');
      this.updateError.set(message);
      return { ok: false, error: message };
    } finally {
      this.updateBusy.set(false);
      this.backupBusy.set(false);
    }
  }

  /** Abre o instalador do Android. O usuário confirma a instalação na tela do sistema. */
  async openAndroidInstaller(): Promise<SettingsActionResult> {
    if (this.updateChannel() !== 'android' || this.updateBusy()) return { ok: false };
    this.updateBusy.set(true);
    try {
      const state = await this.androidUpdate.install();
      this.updatePhase.set(state === 'permissao-necessaria' ? 'permission' : 'ready');
      return { ok: true };
    } catch (error) {
      const message = this.messageOf(error);
      this.updatePhase.set('error');
      this.updateError.set(message);
      return { ok: false, error: message };
    } finally {
      this.updateBusy.set(false);
    }
  }

  async refreshSyncStatus(): Promise<void> {
    this.syncStatus.set(await this.syncService.status());
    const v2 = await this.syncV2.listenState();
    if (v2) this.syncV2State.set(v2);
  }

  /**
   * V1 e V2 não podem estar ativos ao mesmo tempo no mesmo acervo.
   *
   * Decisão registrada: congelar o V1 e substituí-lo, sem coexistir. Um acervo
   * com parte das escritas vindas do snapshot do V1 e parte da causalidade do
   * V2 teria estado cuja origem o V2 não explica. A trava fica na tela, e não
   * no Rust, porque o código do V2 não pode depender do V1.
   */
  syncV1Blocked(): boolean {
    return this.syncV2State().escutando;
  }

  syncV2Blocked(): boolean {
    return this.syncStatus().running;
  }

  async startSyncV2(deviceName: string): Promise<SettingsActionResult> {
    return this.runSyncV2(async () => {
      this.syncV2State.set(await this.syncV2.startListening(this.syncV2Port, deviceName));
    });
  }

  async stopSyncV2(): Promise<SettingsActionResult> {
    return this.runSyncV2(async () => {
      this.syncV2State.set(await this.syncV2.stopListening());
    });
  }

  async newSyncV2Pin(): Promise<SettingsActionResult> {
    return this.runSyncV2(async () => {
      this.syncV2State.set(await this.syncV2.newPin());
    });
  }

  async pairSyncV2(address: string, pin: string, deviceName: string): Promise<{ ok: boolean; result?: SyncSessionResult; error?: string }> {
    const digits = pin.replace(/\D/gu, '');
    if (!address.trim() || digits.length !== 8) {
      return { ok: false, error: 'Informe o endereço e o código de oito dígitos.' };
    }
    return this.sessionSyncV2(() => this.syncV2.pair(address.trim(), digits, deviceName));
  }

  async syncNowV2(address: string, deviceName: string): Promise<{ ok: boolean; result?: SyncSessionResult; error?: string }> {
    if (!address.trim()) return { ok: false, error: 'Informe o endereço do outro aparelho.' };
    return this.sessionSyncV2(() => this.syncV2.syncWith(address.trim(), deviceName));
  }

  private async runSyncV2(action: () => Promise<void>): Promise<SettingsActionResult> {
    if (!isTauri()) return { ok: false, error: 'A sincronização de rede só funciona no aplicativo instalado.' };
    if (this.syncV2Blocked()) return { ok: false, error: 'Pare a sincronização antiga antes de usar a nova.' };
    this.syncV2Busy.set(true);
    try {
      await action();
      return { ok: true };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.syncV2Busy.set(false);
    }
  }

  private async sessionSyncV2(run: () => Promise<SyncSessionResult>): Promise<{ ok: boolean; result?: SyncSessionResult; error?: string }> {
    if (!isTauri()) return { ok: false, error: 'A sincronização de rede só funciona no aplicativo instalado.' };
    if (this.syncV2Blocked()) return { ok: false, error: 'Pare a sincronização antiga antes de usar a nova.' };
    this.syncV2Busy.set(true);
    try {
      const result = await run();
      return { ok: true, result };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.syncV2Busy.set(false);
      const v2 = await this.syncV2.listenState().catch(() => null);
      if (v2) this.syncV2State.set(v2);
    }
  }

  async startSync(deviceName: string): Promise<SettingsActionResult> {
    if (!isTauri()) return { ok: false, error: 'A sincronização de rede só funciona no aplicativo instalado.' };
    this.syncBusy.set(true);
    try {
      this.syncStatus.set(await this.syncService.start(deviceName));
      return { ok: true };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.syncBusy.set(false);
    }
  }

  async stopSync(): Promise<SettingsActionResult> {
    this.syncBusy.set(true);
    try {
      this.syncStatus.set(await this.syncService.stop());
      return { ok: true };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.syncBusy.set(false);
    }
  }

  async connectSync(address: string, code: string, deviceName: string): Promise<{ ok: boolean; result?: SyncResult; error?: string }> {
    if (!isTauri()) return { ok: false, error: 'A sincronização de rede só funciona no aplicativo instalado.' };
    if (!address.trim() || !/^\d{6}$/.test(code.trim())) return { ok: false, error: 'Informe endereço e código de seis dígitos.' };
    this.syncBusy.set(true);
    try {
      const result = await this.syncService.connect(address.trim(), code.trim(), deviceName);
      return { ok: true, result };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.syncBusy.set(false);
    }
  }

  dispose(): void {
    this.updateService.dispose();
  }

  private messageOf(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }
}
