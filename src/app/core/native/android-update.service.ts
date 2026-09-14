import { Injectable } from '@angular/core';
import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { normalizeNativeCommandError } from '../errors/native-command-error';

/** Uma versão mais nova publicada nas GitHub Releases. Sem URL: a tela não escolhe de onde baixar. */
export interface AndroidUpdateNews {
  versao: string;
  titulo: string;
  notas: string;
  tamanhoBytes: number;
  preRelease: boolean;
}

export interface AndroidUpdateCheck {
  suportado: boolean;
  versaoAtual: string;
  novidade: AndroidUpdateNews | null;
}

export interface AndroidUpdateProgress {
  baixados: number;
  total: number;
  percentual: number;
}

export type AndroidInstallerState = 'instalador-aberto' | 'permissao-necessaria';

/**
 * A atualização do NarraHub no Android, por APK assinado das GitHub Releases.
 *
 * Três passos, e nenhum recebe URL, caminho ou hash: o Rust guarda o que ele próprio verificou,
 * confere o SHA-256 no download e de novo antes de abrir o instalador. No desktop a atualização
 * continua sendo a do `tauri-plugin-updater` (`UpdateService`).
 */
@Injectable({ providedIn: 'root' })
export class AndroidUpdateService {
  async supported(): Promise<boolean> {
    if (!isTauri()) return false;
    try {
      return await invoke<boolean>('android_update_supported');
    } catch {
      return false;
    }
  }

  async check(): Promise<AndroidUpdateCheck> {
    return this.call(() => invoke<AndroidUpdateCheck>('android_update_check'), 'Não foi possível verificar se há versão nova.');
  }

  async download(onProgress: (percent: number) => void): Promise<string> {
    const progresso = new Channel<AndroidUpdateProgress>();
    progresso.onmessage = (evento) => onProgress(evento.percentual);
    return this.call(
      () => invoke<string>('android_update_download', { progresso }),
      'Não foi possível baixar a atualização.',
    );
  }

  async install(): Promise<AndroidInstallerState> {
    const resultado = await this.call(
      () => invoke<{ estado: AndroidInstallerState }>('android_update_install'),
      'Não foi possível abrir o instalador do Android.',
    );
    return resultado.estado;
  }

  private async call<T>(run: () => Promise<T>, fallback: string): Promise<T> {
    try {
      return await run();
    } catch (error) {
      throw normalizeNativeCommandError(error, fallback);
    }
  }
}
