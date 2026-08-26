import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { normalizeNativeCommandError } from '../errors/native-command-error';

export type BackupReason = 'manual' | 'pre_migration' | 'pre_update' | 'pre_restore' | 'periodic';

export interface DatabaseHealthIssue {
  code: string;
  message: string;
  count: number;
}

export interface DatabaseHealthReport {
  healthy: boolean;
  checkedAt: string;
  schemaVersion: number;
  integrityResult: string;
  foreignKeyViolations: number;
  issues: DatabaseHealthIssue[];
  tableCounts: Record<string, number>;
}

export interface BackupManifest {
  formatVersion: number;
  backupId: string;
  schemaVersion: number;
  appVersion: string;
  createdAt: string;
  reason: BackupReason;
  database: { file: string; sha256: string; sizeBytes: number };
  assets: { count: number; totalBytes: number; manifestSha256: string; files: Array<{ path: string; sha256: string; sizeBytes: number }> };
}

export interface BackupValidation {
  valid: boolean;
  errors: string[];
  warnings: string[];
  manifest: BackupManifest | null;
  databaseHealth: DatabaseHealthReport | null;
}

export interface RestorePreparation {
  token: string;
  backupId: string;
  safetyBackupId: string;
  schemaVersion: number;
  createdAt: string;
  warnings: string[];
}

export interface RestoreCommitResult {
  restoredBackupId: string;
  safetyBackupId: string;
  rollbackId: string;
  schemaVersion: number;
  requiresRestart: boolean;
}

@Injectable({ providedIn: 'root' })
export class BackupService {
  async health(): Promise<DatabaseHealthReport> {
    this.ensureDesktop();
    return this.invokeDatabase<DatabaseHealthReport>('database_health');
  }

  async create(reason: BackupReason = 'manual'): Promise<BackupManifest> {
    this.ensureDesktop();
    return this.invokeDatabase<BackupManifest>('backup_create', { reason });
  }

  async list(): Promise<BackupManifest[]> {
    this.ensureDesktop();
    return this.invokeDatabase<BackupManifest[]>('backup_list');
  }

  async validate(backupId: string): Promise<BackupValidation> {
    this.ensureDesktop();
    return this.invokeDatabase<BackupValidation>('backup_validate', { backupId });
  }

  async prepareRestore(backupId: string): Promise<RestorePreparation> {
    this.ensureDesktop();
    return this.invokeDatabase<RestorePreparation>('backup_restore_prepare', { backupId });
  }

  async commitRestore(token: string): Promise<RestoreCommitResult> {
    this.ensureDesktop();
    return this.invokeDatabase<RestoreCommitResult>('backup_restore_commit', { token });
  }

  private async invokeDatabase<T>(command: string, args?: Record<string, unknown>): Promise<T> {
    try {
      return await invoke<T>(command, args);
    } catch (error) {
      throw normalizeNativeCommandError(error, 'A operação local não pôde ser concluída.');
    }
  }

  private ensureDesktop(): void {
    if (!isTauri()) throw new Error('Backups locais estão disponíveis somente no aplicativo desktop.');
  }
}
