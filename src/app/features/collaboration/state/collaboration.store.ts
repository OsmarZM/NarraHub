import { Injectable, computed, inject, signal } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';
import { CollaborationContribution, CollaborationSession, SharePermission } from '../models/collaboration.models';
import { OnlineShareDocument, OnlineShareService, OnlineShareStatus, StoredOnlineShare } from '../../../core/native/online-share.service';
import { CollaborationGateway } from '../gateways/collaboration.gateway';

export interface ReviewResult {
  ok: boolean;
  universeId?: string;
  error?: string;
}

@Injectable({ providedIn: 'root' })
export class CollaborationStore {
  private readonly gateway = inject(CollaborationGateway);
  private readonly onlineShareService = inject(OnlineShareService);

  readonly sessions = signal<CollaborationSession[]>([]);
  readonly contributions = signal<CollaborationContribution[]>([]);
  readonly selectedSessionId = signal<string | null>(null);
  readonly pendingCount = computed(() => this.contributions().filter((item) => item.status === 'pending').length);
  readonly selectedContributions = computed(() => {
    const id = this.selectedSessionId();
    return id ? this.contributions().filter((item) => item.session_id === id) : [];
  });
  readonly selectedHasPending = computed(() => this.selectedContributions().some((item) => item.status === 'pending'));

  readonly shareSession = signal<OnlineShareStatus>({ running: false, publicUrl: null, shareCount: 0 });
  readonly shareBusy = signal(false);
  readonly shareProgressMessage = signal('');
  readonly shareLink = signal('');
  readonly shareExpiresAt = signal('');
  readonly onlineShares = signal<StoredOnlineShare[]>([]);

  async refreshShareStatus(): Promise<void> {
    this.shareSession.set(await this.onlineShareService.status());
  }

  async loadReview(): Promise<void> {
    const [sessions, contributions] = await Promise.all([this.gateway.listSessions(), this.gateway.listContributions()]);
    this.sessions.set(sessions);
    this.contributions.set(contributions);
    const selected = this.selectedSessionId();
    if (!selected || !sessions.some((session) => session.id === selected)) {
      this.selectedSessionId.set(sessions[0]?.id ?? null);
    }
  }

  selectSession(id: string): void {
    this.selectedSessionId.set(id);
  }

  async syncIncoming(): Promise<void> {
    if (!isTauri() || !this.onlineShares().length) return;
    let changed = false;
    for (const share of this.onlineShares()) {
      try {
        const incoming = await this.onlineShareService.contributions(share.id, share.revokeToken, share.encryptionKey, share.lastSequence);
        let lastSequence = share.lastSequence;
        for (const item of incoming) {
          lastSequence = Math.max(lastSequence, item.sequence);
          const payload = item.payload;
          const allowed = payload.contributionKind === 'note' ? share.permission !== 'view' : share.permission === 'edit';
          if (!allowed || !share.universeIds.includes(payload.universeId)) continue;
          const stored = await this.gateway.storeContribution(share.id, item.sequence, {
            id: payload.id,
            contributor: payload.contributor,
            kind: payload.contributionKind,
            universeId: payload.universeId,
            targetType: payload.targetType,
            targetId: payload.targetId,
            targetLabel: payload.targetLabel,
            field: payload.field,
            originalValue: payload.originalValue,
            proposedValue: payload.proposedValue,
            message: payload.message,
            createdAt: payload.createdAt,
          });
          changed = stored || changed;
        }
        if (lastSequence !== share.lastSequence) {
          this.onlineShares.update((items) => items.map((item) => item.id === share.id ? { ...item, lastSequence } : item));
        }
      } catch (error) {
        console.warn(`[NarraHub] Não foi possível buscar contribuições da sessão ${share.id}.`, error);
      }
    }
    if (changed) await this.loadReview();
  }

  async review(item: CollaborationContribution, decision: 'approved' | 'rejected'): Promise<ReviewResult> {
    try {
      await this.gateway.review(item.id, decision);
      await this.loadReview();
      return { ok: true, universeId: decision === 'approved' ? item.universe_id : undefined };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    }
  }

  async approveAll(sessionId: string): Promise<{ ok: boolean; count?: number; error?: string }> {
    try {
      const count = await this.gateway.approveAll(sessionId);
      await this.loadReview();
      return { ok: true, count };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    }
  }

  async startShareSession(): Promise<{ ok: boolean; error?: string }> {
    this.shareBusy.set(true);
    this.shareProgressMessage.set('Abrindo um túnel seguro e temporário…');
    try {
      this.shareSession.set(await this.onlineShareService.start());
      return { ok: true };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.shareBusy.set(false);
      this.shareProgressMessage.set('');
    }
  }

  async stopShareSession(): Promise<{ ok: boolean; error?: string }> {
    this.shareBusy.set(true);
    this.shareProgressMessage.set('Encerrando links públicos…');
    try {
      await this.syncIncoming();
      this.shareSession.set(await this.onlineShareService.stop());
      await this.gateway.endAllActive('ended');
      this.onlineShares.set([]);
      this.shareLink.set('');
      this.shareExpiresAt.set('');
      await this.loadReview();
      return { ok: true };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.shareBusy.set(false);
      this.shareProgressMessage.set('');
    }
  }

  async createShare(document: OnlineShareDocument, expiresInDays: number, universeIds: string[]): Promise<{ ok: boolean; error?: string }> {
    this.shareBusy.set(true);
    this.shareProgressMessage.set('Preparando e criptografando somente os itens selecionados…');
    try {
      const created = await this.onlineShareService.create(document, expiresInDays);
      this.shareLink.set(created.url);
      this.shareExpiresAt.set(created.expiresAt);
      this.rememberShare(created.id, created.revokeToken, created.expiresAt, document.title, created.encryptionKey, document.permission, universeIds);
      await this.gateway.saveSession({
        id: created.id, title: document.title, permission: document.permission, universeIds,
        encryptionKey: created.encryptionKey, revokeToken: created.revokeToken, expiresAt: created.expiresAt,
      });
      await this.loadReview();
      this.shareSession.update((status) => ({ ...status, shareCount: status.shareCount + 1 }));
      return { ok: true };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.shareBusy.set(false);
      this.shareProgressMessage.set('');
    }
  }

  async revokeShare(share: StoredOnlineShare): Promise<{ ok: boolean; error?: string }> {
    this.shareBusy.set(true);
    try {
      await this.syncIncoming();
      this.shareSession.set(await this.onlineShareService.revoke(share.id, share.revokeToken));
      await this.gateway.endSession(share.id, 'revoked');
      this.onlineShares.update((items) => items.filter((item) => item.id !== share.id));
      await this.loadReview();
      return { ok: true };
    } catch (error) {
      return { ok: false, error: this.messageOf(error) };
    } finally {
      this.shareBusy.set(false);
    }
  }

  /** Usado só no fechamento do app: encerra sessões sem tocar no estado de UI. */
  async endAllActiveQuietly(): Promise<void> {
    await this.gateway.endAllActive('ended').catch(() => undefined);
  }

  /** Usado só no fechamento do app: para o túnel sem tocar no estado de UI. */
  async stopShareQuietly(): Promise<void> {
    await this.onlineShareService.stop().catch((error) => console.warn('[NarraHub] Falha ao encerrar compartilhamento temporário.', error));
  }

  private rememberShare(id: string, revokeToken: string, expiresAt: string, title: string, encryptionKey: string, permission: SharePermission, universeIds: string[]): void {
    const shares = [{ id, revokeToken, expiresAt, title, encryptionKey, permission, universeIds, lastSequence: 0 }, ...this.onlineShares().filter((item) => item.id !== id)].slice(0, 50);
    this.onlineShares.set(shares);
  }

  private messageOf(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }
}
