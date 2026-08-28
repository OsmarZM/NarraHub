import { Component, OnInit, ViewEncapsulation, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { AiMode, AiModelProfile, AiService } from '../../core/services/ai.service';
import { BackupManifest } from '../../core/services/backup.service';
import { CollaborationContribution, SharePermission } from '../collaboration/models/collaboration.models';
import { StoredOnlineShare } from '../../core/services/online-share.service';
import { ThemeService } from '../../core/services/theme.service';
import { CollaborationStore } from '../collaboration/state/collaboration.store';
import { ProductionReplicaComponent } from '../production-replica/production-replica.component';
import { SettingsStore } from './state/settings.store';

export type SettingsSection = 'general' | 'ai' | 'sync' | 'share' | 'updates';

type RestoreModal = 'restore-backup' | null;

@Component({
  selector: 'app-settings-page',
  standalone: true,
  imports: [FormsModule, ProductionReplicaComponent],
  templateUrl: './settings-page.component.html',
  styleUrl: './settings-page.component.css',
  encapsulation: ViewEncapsulation.None,
})
export class SettingsPageComponent implements OnInit {
  readonly store = inject(SettingsStore);
  readonly collaboration = inject(CollaborationStore);
  readonly ai = inject(AiService);
  readonly theme = inject(ThemeService);

  readonly section = signal<SettingsSection>('general');
  readonly restoreModal = signal<RestoreModal>(null);
  readonly pendingRestoreBackup = signal<BackupManifest | null>(null);
  readonly infoMessage = signal('');
  readonly errorMessage = signal('');

  aiMode: AiMode = this.ai.settings().mode;
  aiEndpoint = this.ai.settings().endpoint;
  aiModel = this.ai.settings().model;
  aiApiKey = this.ai.sessionApiKey;
  aiWriterGuidance = this.ai.writerGuidance();
  aiSelectedProfile: AiModelProfile['id'] = this.ai.localStatus().recommended.id;
  aiInstallBusy = signal(false);
  aiInstallError = signal('');

  deviceName = localStorage.getItem('narrahub.deviceName') || 'Meu computador';
  remoteAddress = '';
  pairingCode = '';
  restoreConfirmation = '';

  ngOnInit(): void {
    this.aiSelectedProfile = (this.ai.localStatus().installedProfile as AiModelProfile['id']) || this.ai.localStatus().recommended.id;
    void this.store.refreshSyncStatus();
    void this.store.refreshBackupStatus();
    void this.store.primeCurrentVersion();
  }

  selectSection(section: SettingsSection): void {
    this.section.set(section);
    if (section === 'general') void this.store.refreshBackupStatus();
  }

  setTheme(value: string): void {
    this.theme.setTheme(value as Parameters<ThemeService['setTheme']>[0]);
  }

  // ── Inteligência artificial ──────────────────────────────

  setAiMode(mode: AiMode): void {
    this.aiMode = mode;
    this.aiInstallError.set('');
    if (mode === 'off') {
      this.ai.disable();
      this.showInfo('Assistência por IA desativada.');
    } else if (mode === 'local') {
      this.aiEndpoint = '';
      this.aiModel = '';
      this.aiSelectedProfile = (this.ai.localStatus().installedProfile as AiModelProfile['id']) || this.ai.localStatus().recommended.id;
    }
  }

  useRecommendedAi(): void {
    this.setAiMode('local');
    this.aiSelectedProfile = this.ai.localStatus().recommended.id;
  }

  aiRecommendationReason(): string {
    const hardware = this.ai.localStatus().hardware;
    const profile = this.ai.localStatus().recommended;
    if (!hardware.totalMemoryGb) return `${profile.name} oferece o melhor equilíbrio estimado para este dispositivo.`;
    const gpu = hardware.gpuMemoryGb ? ` e ${hardware.gpuMemoryGb.toFixed(1)} GB de memória gráfica` : '';
    return `Recomendação calculada com ${hardware.totalMemoryGb.toFixed(1)} GB de RAM, ${hardware.logicalCores} processadores lógicos${gpu} e pontuação ${hardware.score}/100.`;
  }

  async installLocalAi(profile: AiModelProfile['id']): Promise<void> {
    if (this.aiInstallBusy()) return;
    this.aiInstallBusy.set(true);
    this.aiInstallError.set('');
    try {
      await this.ai.installLocal(profile);
      this.aiMode = 'local';
      this.aiSelectedProfile = profile;
      this.showInfo('IA local instalada e iniciada. Ela será carregada automaticamente com o NarraHub.');
    } catch (error) {
      console.error('[NarraHub] A instalação da IA local falhou.', error);
      this.aiInstallError.set(this.messageOf(error));
    } finally {
      this.aiInstallBusy.set(false);
    }
  }

  async activateLocalAi(): Promise<void> {
    if (this.aiInstallBusy()) return;
    this.aiInstallBusy.set(true);
    this.aiInstallError.set('');
    try {
      this.ai.configure({ mode: 'local', endpoint: '', model: '' }, '');
      await this.ai.startLocalEngine(this.ai.localStatus().state === 'error');
      this.aiMode = 'local';
      this.showInfo('IA local iniciada.');
    } catch (error) {
      this.aiInstallError.set(this.messageOf(error));
    } finally {
      this.aiInstallBusy.set(false);
    }
  }

  async restartLocalAi(): Promise<void> {
    if (this.aiInstallBusy()) return;
    this.aiInstallBusy.set(true);
    this.aiInstallError.set('');
    try {
      await this.ai.startLocalEngine(true);
      this.showInfo('IA local reiniciada em modo seguro.');
    } catch (error) {
      this.aiInstallError.set(this.messageOf(error));
    } finally {
      this.aiInstallBusy.set(false);
    }
  }

  formatAiSize(bytes: number): string {
    return bytes >= 1_000_000_000 ? `${(bytes / 1_000_000_000).toFixed(1)} GB` : `${Math.round(bytes / 1_000_000)} MB`;
  }

  selectedAiProfile(): AiModelProfile {
    return this.ai.localStatus().profiles.find((profile) => profile.id === this.aiSelectedProfile) || this.ai.localStatus().recommended;
  }

  installedAiProfile(): AiModelProfile {
    return this.ai.localStatus().profiles.find((profile) => profile.id === this.ai.localStatus().installedProfile) || this.ai.localStatus().recommended;
  }

  saveAiSettings(): void {
    try {
      if (this.aiMode === 'local') { void this.activateLocalAi(); return; }
      this.ai.configure({ mode: this.aiMode, endpoint: this.aiEndpoint, model: this.aiModel }, this.aiApiKey);
      this.aiEndpoint = this.ai.settings().endpoint;
      this.showInfo(this.aiMode === 'off' ? 'Assistência por IA desativada.' : 'API própria configurada para esta sessão.');
    } catch (error) {
      this.showError(this.messageOf(error));
    }
  }

  saveWriterGuidance(): void {
    this.ai.setWriterGuidance(this.aiWriterGuidance);
    this.showInfo('Perfil criativo salvo somente neste dispositivo.');
  }

  // ── Backup e integridade ─────────────────────────────────

  async requestManualBackup(): Promise<void> {
    const result = await this.store.createBackup('manual');
    if (result.ok) this.showInfo('Backup local criado e validado.');
    else if (result.error) this.showError(result.error);
  }

  async validateBackup(backupId: string): Promise<void> {
    const result = await this.store.validateBackup(backupId);
    if (result.ok) this.showInfo('Backup íntegro e compatível com o manifesto.');
  }

  requestRestoreBackup(backup: BackupManifest): void {
    this.pendingRestoreBackup.set(backup);
    this.store.clearRestorePreparation();
    this.restoreConfirmation = '';
    this.restoreModal.set('restore-backup');
  }

  closeRestoreModal(): void {
    if (this.store.backupBusy()) return;
    this.restoreModal.set(null);
    this.pendingRestoreBackup.set(null);
  }

  async requestPrepareRestore(): Promise<void> {
    const backup = this.pendingRestoreBackup();
    if (!backup) return;
    if (this.collaboration.shareSession().running || this.store.syncStatus().running) {
      this.store.backupError.set('Encerre o compartilhamento e a sincronização antes de restaurar um backup.');
      return;
    }
    const result = await this.store.prepareRestore(backup.backupId);
    if (!result.ok && result.error) this.showError(result.error);
  }

  async confirmRestore(): Promise<void> {
    const preparation = this.store.restorePreparation();
    if (!preparation || this.restoreConfirmation.trim() !== 'RESTAURAR' || this.store.backupBusy()) return;
    const result = await this.store.commitRestore(preparation.token);
    if (!result.ok) this.showError(result.error || 'Não foi possível concluir a restauração recuperável.');
  }

  // ── Atualizações ─────────────────────────────────────────

  async requestUpdateCheck(): Promise<void> {
    const result = await this.store.checkForUpdates(false);
    if (result.message) result.ok ? this.showInfo(result.message) : this.showError(result.message);
  }

  async requestUpdateInstall(): Promise<void> {
    const result = await this.store.installUpdate();
    if (!result.ok && result.error) this.showError(result.error);
  }

  dismissUpdatePrompt(): void {
    this.store.dismissUpdatePrompt();
  }

  // ── Sincronização entre dispositivos ─────────────────────

  saveDeviceName(): void {
    this.deviceName = this.deviceName.trim() || 'Meu computador';
    localStorage.setItem('narrahub.deviceName', this.deviceName);
    this.showInfo('Nome do dispositivo salvo.');
  }

  async startSync(): Promise<void> {
    this.saveDeviceName();
    const result = await this.store.startSync(this.deviceName);
    if (!result.ok && result.error) this.showError(result.error);
  }

  async stopSync(): Promise<void> {
    const result = await this.store.stopSync();
    if (!result.ok && result.error) this.showError(result.error);
  }

  async connectSync(): Promise<void> {
    const result = await this.store.connectSync(this.remoteAddress, this.pairingCode, this.deviceName);
    if (!result.ok) { if (result.error) this.showError(result.error); return; }
    const peer = result.result;
    if (peer) this.showInfo(`Sincronizado com ${peer.peer_name}: ${peer.received} recebidos, ${peer.sent} enviados, ${peer.conflicts} conflitos.`);
  }

  // ── Compartilhamento e colaboração ───────────────────────

  async startShare(): Promise<void> {
    const result = await this.collaboration.startShareSession();
    if (result.ok) this.showInfo('Compartilhamento temporário disponível enquanto o NarraHub estiver aberto.');
    else this.showError(result.error || 'Não foi possível abrir o compartilhamento temporário.');
  }

  async stopShare(): Promise<void> {
    const result = await this.collaboration.stopShareSession();
    if (result.ok) this.showInfo('Sessão encerrada. Todos os links temporários foram invalidados.');
    else this.showError(result.error || 'Não foi possível encerrar o compartilhamento.');
  }

  async revokeShare(share: StoredOnlineShare): Promise<void> {
    const result = await this.collaboration.revokeShare(share);
    if (result.ok) this.showInfo('Compartilhamento revogado. O link não pode mais ser aberto.');
    else this.showError(result.error || 'Não foi possível revogar o compartilhamento.');
  }

  async reviewContribution(item: CollaborationContribution, decision: 'approved' | 'rejected'): Promise<void> {
    const result = await this.collaboration.review(item, decision);
    if (!result.ok) { this.showError(result.error || 'Não foi possível revisar a alteração colaborativa.'); return; }
    this.showInfo(decision === 'approved' ? 'Alteração aprovada e aplicada ao banco local.' : 'Alteração rejeitada e preservada no histórico da sessão.');
  }

  async approveAllContributions(sessionId: string): Promise<void> {
    const result = await this.collaboration.approveAll(sessionId);
    if (!result.ok) { this.showError(result.error || 'Não foi possível aprovar as alterações em lote.'); return; }
    this.showInfo(`${result.count} alteração(ões) aprovada(s) e aplicada(s).`);
  }

  sharePermissionLabel(permission: SharePermission): string {
    return permission === 'edit' ? 'Pode propor edições' : permission === 'comment' ? 'Somente anotações' : 'Somente leitura';
  }

  contributionFieldLabel(field: string): string {
    if (field.startsWith('attribute:')) return field.slice('attribute:'.length);
    return ({ name: 'Nome', description: 'Descrição', title: 'Título', content: 'Texto', summary: 'Resumo', canon_status: 'Estado canônico' } as Record<string, string>)[field] || field;
  }

  formatDate(value: string): string {
    if (!value) return 'Sem data';
    const date = new Date(value.length === 10 ? `${value}T12:00:00` : value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString('pt-BR', { day: '2-digit', month: 'short', year: 'numeric' });
  }

  private showInfo(message: string): void {
    this.errorMessage.set('');
    this.infoMessage.set(message);
  }

  private showError(message: string): void {
    this.infoMessage.set('');
    this.errorMessage.set(message);
  }

  private messageOf(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }
}
