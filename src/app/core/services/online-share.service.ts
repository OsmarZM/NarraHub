import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';

export interface SharedChapter {
  id: string;
  title: string;
  content: string;
  universeName: string;
  storyName: string;
  bookName: string;
}

export interface SharedEntity {
  id: string;
  type: string;
  name: string;
  description: string;
  image: string;
  canonStatus: string;
  attributes: Array<{ key: string; value: string }>;
}

export interface OnlineShareDocument {
  version: 2;
  kind: 'bundle';
  title: string;
  universe: { name: string; description: string; coverImage: string } | null;
  chapters: SharedChapter[];
  entities: SharedEntity[];
  sharedAt: string;
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
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const plaintext = new TextEncoder().encode(JSON.stringify(document));
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
    });
    const viewerUrl = new URL(created.url);
    if (viewerUrl.protocol !== 'https:' || !viewerUrl.hostname.endsWith('.trycloudflare.com')) {
      throw new Error('O túnel retornou um endereço público inválido.');
    }
    viewerUrl.hash = `k=${this.toBase64Url(new Uint8Array(rawKey))}`;
    return { ...created, url: viewerUrl.toString() };
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

  private toBase64Url(bytes: Uint8Array): string {
    let binary = '';
    for (let index = 0; index < bytes.length; index += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
    }
    return btoa(binary).replace(/\+/gu, '-').replace(/\//gu, '_').replace(/=+$/gu, '');
  }
}
