import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';

export interface SharedChapter {
  id: string;
  title: string;
  content: string;
  storyName: string;
  bookName: string;
  summary: string;
}

export interface SharedEntity {
  id: string;
  type: string;
  name: string;
  summary: string;
  description: string;
  image: string;
  canonStatus: string;
  attributes: Array<{ key: string; value: string }>;
}

export interface SharedUniverse {
  id: string;
  name: string;
  description: string;
  coverImage: string;
  chapters: SharedChapter[];
  entities: SharedEntity[];
}

export type SharePermission = 'view' | 'comment' | 'edit';

export interface OnlineShareDocument {
  version: 3;
  kind: 'workspace';
  title: string;
  permission: SharePermission;
  collaborationToken?: string;
  universes: SharedUniverse[];
  sharedAt: string;
}

export interface StoredOnlineShare {
  id: string;
  revokeToken: string;
  expiresAt: string;
  title: string;
  encryptionKey: string;
  permission: SharePermission;
  universeIds: string[];
  lastSequence: number;
}

interface EncryptedShareEnvelope {
  version: 1;
  algorithm: 'A256GCM';
  iv: string;
  ciphertext: string;
}

export interface CreatedOnlineShare {
  id: string;
  url: string;
  expiresAt: string;
  revokeToken: string;
  encryptionKey: string;
}

export interface EncryptedContributionRecord {
  sequence: number;
  envelope: EncryptedShareEnvelope;
  receivedAt: string;
}

export interface ShareContributionPayload {
  version: 1;
  kind: 'contribution';
  id: string;
  contributor: string;
  contributionKind: 'edit' | 'note';
  universeId: string;
  targetType: 'universe' | 'chapter' | 'entity';
  targetId: string;
  targetLabel: string;
  field?: string;
  originalValue?: string;
  proposedValue?: string;
  message?: string;
  createdAt: string;
}

export interface OnlineShareStatus {
  running: boolean;
  publicUrl: string | null;
  shareCount: number;
}

@Injectable({ providedIn: 'root' })
export class OnlineShareService {
  async status(): Promise<OnlineShareStatus> {
    if (!isTauri()) return { running: false, publicUrl: null, shareCount: 0 };
    return invoke<OnlineShareStatus>('online_share_status');
  }

  async start(): Promise<OnlineShareStatus> {
    this.requireNativeApp();
    return invoke<OnlineShareStatus>('online_share_start');
  }

  async stop(): Promise<OnlineShareStatus> {
    if (!isTauri()) return { running: false, publicUrl: null, shareCount: 0 };
    return invoke<OnlineShareStatus>('online_share_stop');
  }

  async create(document: OnlineShareDocument, expiresInDays: number): Promise<CreatedOnlineShare> {
    this.requireNativeApp();
    const key = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, true, ['encrypt']);
    const contributionToken = this.toBase64Url(crypto.getRandomValues(new Uint8Array(32)));
    const securedDocument = { ...document, collaborationToken: contributionToken };
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const plaintext = new TextEncoder().encode(JSON.stringify(securedDocument));
    if (plaintext.byteLength > 2_000_000) {
      throw new Error('A seleção ficou acima de 2 MB. Remova algumas imagens ou divida o compartilhamento.');
    }
    const encrypted = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, plaintext);
    const rawKey = await crypto.subtle.exportKey('raw', key);
    const envelope: EncryptedShareEnvelope = {
      version: 1,
      algorithm: 'A256GCM',
      iv: this.toBase64Url(iv),
      ciphertext: this.toBase64Url(new Uint8Array(encrypted)),
    };

    const created = await invoke<CreatedOnlineShare>('online_share_create', {
      envelope,
      expiresInDays: this.normalizeExpiry(expiresInDays),
      contributionToken,
    });
    const viewerUrl = new URL(created.url);
    if (viewerUrl.protocol !== 'https:' || !viewerUrl.hostname.endsWith('.trycloudflare.com')) {
      throw new Error('O túnel retornou um endereço público inválido.');
    }
    viewerUrl.hash = `k=${this.toBase64Url(new Uint8Array(rawKey))}`;
    return { ...created, url: viewerUrl.toString(), encryptionKey: this.toBase64Url(new Uint8Array(rawKey)) };
  }

  async contributions(id: string, revokeToken: string, encryptionKey: string, afterSequence: number): Promise<Array<{ sequence: number; payload: ShareContributionPayload }>> {
    this.requireNativeApp();
    const records = await invoke<EncryptedContributionRecord[]>('online_share_contributions', {
      id, revokeToken, afterSequence,
    });
    const results: Array<{ sequence: number; payload: ShareContributionPayload }> = [];
    for (const record of records) {
      const payload = await this.decryptContribution(record.envelope, encryptionKey);
      if (payload.version === 1 && payload.kind === 'contribution') results.push({ sequence: record.sequence, payload });
    }
    return results;
  }

  async revoke(id: string, revokeToken: string): Promise<OnlineShareStatus> {
    this.requireNativeApp();
    return invoke<OnlineShareStatus>('online_share_revoke', { id, revokeToken });
  }

  private requireNativeApp(): void {
    if (!isTauri()) throw new Error('O compartilhamento temporário funciona somente no aplicativo instalado.');
  }

  private normalizeExpiry(value: number): number {
    return [1, 7, 30].includes(value) ? value : 7;
  }

  private async decryptContribution(envelope: EncryptedShareEnvelope, encryptionKey: string): Promise<ShareContributionPayload> {
    const key = await crypto.subtle.importKey('raw', this.fromBase64Url(encryptionKey), { name: 'AES-GCM' }, false, ['decrypt']);
    const plaintext = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: this.fromBase64Url(envelope.iv) },
      key,
      this.fromBase64Url(envelope.ciphertext),
    );
    return JSON.parse(new TextDecoder().decode(plaintext)) as ShareContributionPayload;
  }

  private toBase64Url(bytes: Uint8Array): string {
    let binary = '';
    for (let index = 0; index < bytes.length; index += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
    }
    return btoa(binary).replace(/\+/gu, '-').replace(/\//gu, '_').replace(/=+$/gu, '');
  }


  private fromBase64Url(value: string): Uint8Array<ArrayBuffer> {
    const normalized = value.replace(/-/gu, '+').replace(/_/gu, '/');
    const binary = atob(normalized + '='.repeat((4 - normalized.length % 4) % 4));
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
    return bytes;
  }
}
