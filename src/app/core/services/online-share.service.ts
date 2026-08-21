import { Injectable } from '@angular/core';

export interface OnlineShareDocument {
  version: 1;
  kind: 'chapter';
  title: string;
  content: string;
  universeName: string;
  storyName: string;
  bookName: string;
  sharedAt: string;
}

interface EncryptedShareEnvelope {
  version: 1;
  algorithm: 'A256GCM';
  iv: string;
  ciphertext: string;
  expiresInDays: number;
}

interface CreateShareResponse {
  id: string;
  url: string;
  expiresAt: string;
  revokeToken: string;
}

export interface CreatedOnlineShare {
  id: string;
  url: string;
  expiresAt: string;
  revokeToken: string;
}

export interface OnlineShareHealth {
  ok: boolean;
  service: string;
  encryption: string;
}

@Injectable({ providedIn: 'root' })
export class OnlineShareService {
  async health(apiUrl: string): Promise<OnlineShareHealth> {
    const baseUrl = this.normalizeApiUrl(apiUrl);
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), 8_000);
    try {
      const response = await fetch(`${baseUrl}/health`, { cache: 'no-store', signal: controller.signal });
      if (!response.ok) throw new Error(`Servidor respondeu com status ${response.status}.`);
      const health = await response.json() as OnlineShareHealth;
      if (!health.ok || health.service !== 'narrahub-share') {
        throw new Error('O endereço não aponta para um servidor NarraHub Share compatível.');
      }
      return health;
    } catch (error) {
      if (error instanceof DOMException && error.name === 'AbortError') {
        throw new Error('O servidor não respondeu em até 8 segundos.');
      }
      throw error;
    } finally {
      window.clearTimeout(timeout);
    }
  }

  async create(apiUrl: string, document: OnlineShareDocument, expiresInDays: number): Promise<CreatedOnlineShare> {
    const baseUrl = this.normalizeApiUrl(apiUrl);
    const key = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, true, ['encrypt']);
    const iv = crypto.getRandomValues(new Uint8Array(12));
    const plaintext = new TextEncoder().encode(JSON.stringify(document));
    const encrypted = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, plaintext);
    const rawKey = await crypto.subtle.exportKey('raw', key);
    const envelope: EncryptedShareEnvelope = {
      version: 1,
      algorithm: 'A256GCM',
      iv: this.toBase64Url(iv),
      ciphertext: this.toBase64Url(new Uint8Array(encrypted)),
      expiresInDays: this.normalizeExpiry(expiresInDays),
    };

    const response = await fetch(`${baseUrl}/v1/shares`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(envelope),
    });
    if (!response.ok) {
      const detail = await response.text().catch(() => '');
      throw new Error(`Servidor recusou o compartilhamento (${response.status})${detail ? `: ${detail.slice(0, 180)}` : ''}`);
    }

    const created = await response.json() as CreateShareResponse;
    const viewerUrl = new URL(created.url);
    if (!['https:', 'http:'].includes(viewerUrl.protocol) || viewerUrl.origin !== new URL(baseUrl).origin) {
      throw new Error('O servidor retornou um endereço de compartilhamento inválido.');
    }
    viewerUrl.hash = `k=${this.toBase64Url(new Uint8Array(rawKey))}`;
    return { ...created, url: viewerUrl.toString() };
  }

  async revoke(apiUrl: string, id: string, revokeToken: string): Promise<void> {
    const baseUrl = this.normalizeApiUrl(apiUrl);
    const response = await fetch(`${baseUrl}/v1/shares/${encodeURIComponent(id)}`, {
      method: 'DELETE',
      headers: { authorization: `Bearer ${revokeToken}` },
    });
    if (response.ok || response.status === 404) return;
    if (response.status === 403) throw new Error('O servidor recusou o token de revogação deste link.');
    throw new Error(`Não foi possível revogar o compartilhamento (${response.status}).`);
  }

  normalizeApiUrl(value: string): string {
    const trimmed = value.trim().replace(/\/+$/u, '');
    if (!trimmed) throw new Error('Configure o servidor de compartilhamento online.');
    const url = new URL(trimmed);
    const isLocal = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
    if (url.protocol !== 'https:' && !(isLocal && url.protocol === 'http:')) {
      throw new Error('O servidor online precisa usar HTTPS. HTTP é aceito somente no desenvolvimento local.');
    }
    if (url.username || url.password || url.search || url.hash) {
      throw new Error('Informe apenas o endereço base do servidor.');
    }
    return url.toString().replace(/\/$/u, '');
  }

  private normalizeExpiry(value: number): number {
    if (![1, 7, 30].includes(value)) return 7;
    return value;
  }

  private toBase64Url(bytes: Uint8Array): string {
    let binary = '';
    for (let index = 0; index < bytes.length; index += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
    }
    return btoa(binary).replace(/\+/gu, '-').replace(/\//gu, '_').replace(/=+$/gu, '');
  }
}
