import { Injectable } from '@angular/core';
import { getVersion } from '@tauri-apps/api/app';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';

export interface AppUpdateInfo {
  currentVersion: string;
  availableVersion: string | null;
  notes: string;
  publishedAt: string | null;
}

@Injectable({ providedIn: 'root' })
export class UpdateService {
  private pendingUpdate: Update | null = null;

  async currentVersion(): Promise<string> {
    return isTauri() ? getVersion() : '0.3.1-dev';
  }

  async isConfigured(): Promise<boolean> {
    return isTauri() && invoke<boolean>('updater_configured');
  }

  async check(): Promise<AppUpdateInfo> {
    const currentVersion = await this.currentVersion();
    if (!isTauri()) return { currentVersion, availableVersion: null, notes: '', publishedAt: null };
    this.pendingUpdate?.close();
    this.pendingUpdate = await check({ timeout: 15_000 });
    return {
      currentVersion,
      availableVersion: this.pendingUpdate?.version || null,
      notes: this.pendingUpdate?.body || '',
      publishedAt: this.pendingUpdate?.date || null,
    };
  }

  async downloadAndInstall(onProgress: (progress: number) => void): Promise<void> {
    if (!this.pendingUpdate) throw new Error('Nenhuma atualização está pronta para instalar.');
    let downloaded = 0;
    let total = 0;
    await this.pendingUpdate.downloadAndInstall((event) => {
      if (event.event === 'Started') total = event.data.contentLength || 0;
      if (event.event === 'Progress') downloaded += event.data.chunkLength;
      if (event.event === 'Finished') onProgress(100);
      else if (total > 0) onProgress(Math.min(99, Math.round(downloaded / total * 100)));
    });
  }

  async relaunch(): Promise<void> {
    await relaunch();
  }

  dispose(): void {
    this.pendingUpdate?.close();
    this.pendingUpdate = null;
  }
}
