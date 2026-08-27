import { Component, EventEmitter, Input, OnInit, Output, ViewEncapsulation, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { AiMode, AiModelProfile, AiService } from '../../core/services/ai.service';
import { BackupManifest } from '../../core/services/backup.service';
import { CollaborationContribution, CollaborationSession, SharePermission } from '../../core/services/collaboration.service';
import { OnlineShareStatus, StoredOnlineShare } from '../../core/services/online-share.service';
import { ThemeService } from '../../core/services/theme.service';
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
  // A memória criativa da IA é isolada por universo (Assistance depende de
  // Workspace); o universo ativo em si continua vivendo no AppState.
  @Input() activeUniverseId: string | null = null;

  // Colaboração e compartilhamento ainda não foram extraídos do App (isso é a
  // próxima fatia da Fase 2); a aba "Compartilhar" só exibe o que o App já
  // orquestra e devolve as ações por evento, sem duplicar o domínio aqui.
  @Input() shareSession: OnlineShareStatus = { running: false, publicUrl: null, shareCount: 0 };
  @Input() shareBusy = false;
  @Input() onlineShares: StoredOnlineShare[] = [];
  @Input() collaborationSessions: CollaborationSession[] = [];
  @Input() selectedCollaborationSessionId: string | null = null;
  @Input() selectedCollaborationHasPending = false;
  @Input() selectedCollaborationContributions: CollaborationContribution[] = [];
  @Input() pendingCollaborationCount = 0;

  @Output() readonly info = new EventEmitter<string>();
  @Output() readonly failed = new EventEmitter<string>();
  @Output() readonly backupCreateRequested = new EventEmitter<void>();
  @Output() readonly restorePrepareRequested = new EventEmitter<BackupManifest>();
  @Output() readonly updateCheckRequested = new EventEmitter<void>();
  @Output() readonly updateInstallRequested = new EventEmitter<void>();
  @Output() readonly synced = new EventEmitter<void>();
  @Output() readonly shareStartRequested = new EventEmitter<void>();
  @Output() readonly shareStopRequested = new EventEmitter<void>();
  @Output() readonly shareRevokeRequested = new EventEmitter<StoredOnlineShare>();
  @Output() readonly collaborationSessionSelected = new EventEmitter<string>();
  @Output() readonly collaborationApproveAllRequested = new EventEmitter<string>();
  @Output() readonly collaborationReviewRequested = new EventEmitter<{ item: CollaborationContribution; decision: 'approved' | 'rejected' }>();

  readonly store = inject(SettingsStore);
  readonly ai = inject(AiService);
  readonly theme = inject(ThemeService);

  readonly section = signal<SettingsSection>('general');
  readonly restoreModal = signal<RestoreModal>(null);
  readonly pendingRestoreBackup = signal<BackupManifest | null>(null);

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
      this.info.emit('Assistência por IA desativada.');
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
      this.info.emit('IA local instalada e iniciada. Ela será carregada automaticamente com o NarraHub.');
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
      this.info.emit('IA local iniciada.');
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
      this.info.emit('IA local reiniciada em modo seguro.');
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
      this.info.emit(this.aiMode === 'off' ? 'Assistência por IA desativada.' : 'API própria configurada para esta sessão.');
    } catch (error) {
      this.failed.emit(this.messageOf(error));
    }
  }

  saveWriterGuidance(): void {
    this.ai.setWriterGuidance(this.aiWriterGuidance);
    this.info.emit('Perfil criativo salvo somente neste dispositivo.');
  }

  clearAiLearning(universeId: string | null): void {
    if (!universeId || !window.confirm('Esquecer as decisões de IA registradas neste universo?')) return;
    this.ai.forgetCreativeMemory(universeId);
    this.info.emit('Memória de decisões deste universo removida.');
  }

  aiMemoryCount(universeId: string | null): number {
    return universeId ? this.ai.creativeMemory().filter((item) => item.scope === universeId).length : 0;
  }

  // ── Backup e integridade ─────────────────────────────────

  requestManualBackup(): void {
    this.backupCreateRequested.emit();
  }

  async validateBackup(backupId: string): Promise<void> {
    const result = await this.store.validateBackup(backupId);
    if (result.ok) this.info.emit('Backup íntegro e compatível com o manifesto.');
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

  requestPrepareRestore(): void {
    const backup = this.pendingRestoreBackup();
    if (backup) this.restorePrepareRequested.emit(backup);
  }

  async confirmRestore(): Promise<void> {
    const preparation = this.store.restorePreparation();
    if (!preparation || this.restoreConfirmation.trim() !== 'RESTAURAR' || this.store.backupBusy()) return;
    const result = await this.store.commitRestore(preparation.token);
    if (!result.ok) this.failed.emit(result.error || 'Não foi possível concluir a restauração recuperável.');
  }

  // ── Atualizações ─────────────────────────────────────────

  requestUpdateCheck(): void {
    this.updateCheckRequested.emit();
  }

  requestUpdateInstall(): void {
    this.updateInstallRequested.emit();
  }

  dismissUpdatePrompt(): void {
    this.store.dismissUpdatePrompt();
  }

  // ── Sincronização entre dispositivos ─────────────────────

  saveDeviceName(): void {
    this.deviceName = this.deviceName.trim() || 'Meu computador';
    localStorage.setItem('narrahub.deviceName', this.deviceName);
    this.info.emit('Nome do dispositivo salvo.');
  }

  async startSync(): Promise<void> {
    this.saveDeviceName();
    const result = await this.store.startSync(this.deviceName);
    if (!result.ok && result.error) this.failed.emit(result.error);
  }

  async stopSync(): Promise<void> {
    const result = await this.store.stopSync();
    if (!result.ok && result.error) this.failed.emit(result.error);
  }

  async connectSync(): Promise<void> {
    const result = await this.store.connectSync(this.remoteAddress, this.pairingCode, this.deviceName);
    if (!result.ok) { if (result.error) this.failed.emit(result.error); return; }
    this.synced.emit();
    const peer = result.result;
    if (peer) this.info.emit(`Sincronizado com ${peer.peer_name}: ${peer.received} recebidos, ${peer.sent} enviados, ${peer.conflicts} conflitos.`);
  }

  // ── Compartilhamento e colaboração (repassado ao App) ────

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

  private messageOf(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }
}
