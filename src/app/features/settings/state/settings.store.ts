import { Injectable, inject, signal } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';
import { SyncResult, SyncServerStatus } from '../../../core/models';
import { BackupManifest, BackupService, BackupValidation, DatabaseHealthReport, RestorePreparation } from '../../../core/services/backup.service';
// DatabaseService is injected here on purpose, unlike the domain gateways: restoring a
// backup has to close and reopen the app's own SQLite connection pool, which is native
// pool lifecycle, not the SQL-vs-Rust boundary the other LegacyXGateway adapters abstract.
import { DatabaseService } from '../../../core/services/database.service';
import { SyncService } from '../../../core/services/sync.service';
import { AppUpdateInfo, UpdateService } from '../../../core/services/update.service';

export type UpdatePhase = 'idle' | 'checking' | 'available' | 'backing-up' | 'downloading' | 'current' | 'error';

export interface SettingsActionResult {
  ok: boolean;
  error?: string;
}

@Injectable({ providedIn: 'root' })
export class SettingsStore {
  private readonly backupService = inject(BackupService);
  private readonly updateService = inject(UpdateService);
  private readonly syncService = inject(SyncService);
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

  readonly syncStatus = signal<SyncServerStatus>({ running: false, address: null, pairing_code: null, device_name: 'Meu computador' });
  readonly syncBusy = signal(false);

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
  }

  isUpdateConfigured(): Promise<boolean> {
    return this.updateService.isConfigured();
  }

  async checkForUpdates(silent: boolean): Promise<{ ok: boolean; message: string }> {
    if (!isTauri()) return { ok: false, message: silent ? '' : 'A atualização automática funciona somente no aplicativo instalado.' };
    if (this.updateBusy()) return { ok: false, message: '' };
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

  async refreshSyncStatus(): Promise<void> {
    this.syncStatus.set(await this.syncService.status());
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
