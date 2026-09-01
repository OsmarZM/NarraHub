import { Injectable } from '@angular/core';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { SyncResult, SyncServerStatus } from '../models';

@Injectable({ providedIn: 'root' })
export class SyncService {
  async status(): Promise<SyncServerStatus> {
    if (!isTauri()) return { running: false, address: null, pairing_code: null, device_name: 'Prévia no navegador' };
    return invoke<SyncServerStatus>('sync_status');
  }

  async start(deviceName: string): Promise<SyncServerStatus> {
    return invoke<SyncServerStatus>('sync_start', { deviceName });
  }

  async stop(): Promise<SyncServerStatus> {
    return invoke<SyncServerStatus>('sync_stop');
  }

  async connect(address: string, code: string, deviceName: string): Promise<SyncResult> {
    return invoke<SyncResult>('sync_connect', { address, code, deviceName });
  }
}
