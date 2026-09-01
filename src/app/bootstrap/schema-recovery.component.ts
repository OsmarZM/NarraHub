import { Component, Input, computed, inject, signal } from '@angular/core';
import { BackupManifest, BackupService } from '../core/native/backup.service';
import { UpdateService } from '../core/native/update.service';
import { DatabaseCompatibility } from './database-compatibility';

/**
 * Tela mostrada quando o banco no disco é mais novo do que este executável entende.
 *
 * Existe por causa de um incidente real: instalar uma versão antiga sobre um banco novo
 * fazia o aplicativo simplesmente não abrir — sem janela, sem mensagem — enquanto os dados
 * estavam intactos o tempo todo. Ver ADR 0007.
 *
 * Ela não depende do `DatabaseService`: o pool nunca é aberto neste estado. Fala apenas com
 * `BackupService` e `UpdateService`, que são comandos Rust e funcionam sem o pool.
 */
@Component({
  selector: 'app-schema-recovery',
  standalone: true,
  templateUrl: './schema-recovery.component.html',
  styleUrl: './schema-recovery.component.css',
})
export class SchemaRecoveryComponent {
  @Input({ required: true }) compatibility!: DatabaseCompatibility;

  private readonly backupService = inject(BackupService);
  private readonly updateService = inject(UpdateService);

  readonly busy = signal(false);
  readonly message = signal('');
  readonly error = signal('');
  readonly progress = signal(0);
  readonly backups = signal<BackupManifest[]>([]);
  readonly backupsLoaded = signal(false);
  readonly confirmingBackupId = signal('');

  /**
   * Só backups que este executável consegue abrir. Oferecer um backup que também não abre
   * seria repetir o problema dentro da própria solução.
   */
  readonly compatibleBackups = computed(() =>
    this.backups().filter((backup) => backup.schemaVersion <= this.compatibility.supportedSchemaVersion),
  );

  readonly incompatibleBackupCount = computed(() => this.backups().length - this.compatibleBackups().length);

  async searchUpdate(): Promise<void> {
    if (this.busy()) return;
    this.start('Procurando uma versão mais nova…');
    try {
      if (!(await this.updateService.isConfigured())) {
        this.error.set(
          'Este build não tem canal de atualização configurado. Baixe a versão mais recente pelo site e instale por cima.',
        );
        return;
      }
      const info = await this.updateService.check();
      if (!info.availableVersion) {
        this.error.set(
          'Nenhuma atualização foi oferecida. Instale manualmente uma versão igual ou mais nova que a que gerou estes dados.',
        );
        return;
      }
      this.message.set(`Instalando NarraHub ${info.availableVersion}…`);
      await this.updateService.downloadAndInstall((progress) => this.progress.set(progress));
      await this.updateService.relaunch();
    } catch (error) {
      this.error.set(this.messageOf(error));
    } finally {
      this.busy.set(false);
    }
  }

  async loadBackups(): Promise<void> {
    if (this.busy()) return;
    this.start('Lendo os backups deste dispositivo…');
    try {
      this.backups.set(await this.backupService.list());
      this.backupsLoaded.set(true);
      this.message.set('');
    } catch (error) {
      this.error.set(this.messageOf(error));
    } finally {
      this.busy.set(false);
    }
  }

  async restore(backupId: string): Promise<void> {
    if (this.busy()) return;
    this.start('Validando e restaurando o backup…');
    try {
      const preparation = await this.backupService.prepareRestore(backupId);
      await this.backupService.commitRestore(preparation.token);
      this.message.set('Backup restaurado. Reiniciando o NarraHub…');
      await this.updateService.relaunch();
    } catch (error) {
      this.error.set(this.messageOf(error));
    } finally {
      this.busy.set(false);
      this.confirmingBackupId.set('');
    }
  }

  formatDate(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString('pt-BR');
  }

  formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  private start(message: string): void {
    this.busy.set(true);
    this.error.set('');
    this.progress.set(0);
    this.message.set(message);
  }

  private messageOf(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }
}
