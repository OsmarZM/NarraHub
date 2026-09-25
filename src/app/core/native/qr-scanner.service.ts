import { Injectable } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';

/**
 * O que a leitura de QR devolveu. `conteudo` é a string crua do código — quem interpreta é o Rust
 * (`sync_v2_parear_por_qr`); esta porta não conhece o formato.
 */
export type QrScanResult =
  | { kind: 'ok'; conteudo: string }
  | { kind: 'cancelado' }
  | { kind: 'negado' }
  | { kind: 'indisponivel' };

/**
 * Porta de plataforma: a câmera do celular lendo um QR (NH-084, PR B).
 *
 * É plataforma e não domínio: aciona um recurso do aparelho e devolve texto cru. O plugin só existe no
 * Android e no iOS; em qualquer outro lugar a resposta é `indisponivel`, e a tela continua com o
 * endereço e o código digitados — a leitura é atalho, nunca o único caminho.
 *
 * Nada aqui registra o conteúdo lido: ele carrega o PIN.
 */
@Injectable({ providedIn: 'root' })
export class QrScannerService {
  /** Se há leitor neste aparelho. Detecção barata, sem pedir permissão. */
  async supported(): Promise<boolean> {
    if (!isTauri()) return false;
    try {
      const plugin = await import('@tauri-apps/plugin-barcode-scanner');
      await plugin.checkPermissions();
      return true;
    } catch {
      return false;
    }
  }

  async scan(): Promise<QrScanResult> {
    if (!isTauri()) return { kind: 'indisponivel' };
    let plugin: typeof import('@tauri-apps/plugin-barcode-scanner');
    try {
      plugin = await import('@tauri-apps/plugin-barcode-scanner');
    } catch {
      return { kind: 'indisponivel' };
    }
    try {
      let permissao = await plugin.checkPermissions();
      if (permissao !== 'granted') permissao = await plugin.requestPermissions();
      if (permissao !== 'granted') return { kind: 'negado' };
      const lido = await plugin.scan({ formats: [plugin.Format.QRCode], windowed: false });
      const conteudo = lido?.content ?? '';
      return conteudo ? { kind: 'ok', conteudo } : { kind: 'cancelado' };
    } catch (error) {
      // O plugin rejeita quando o usuário volta sem ler; sem código lido, é cancelamento.
      return String(error ?? '').toLowerCase().includes('permission') ? { kind: 'negado' } : { kind: 'cancelado' };
    }
  }

  async cancel(): Promise<void> {
    if (!isTauri()) return;
    try {
      const plugin = await import('@tauri-apps/plugin-barcode-scanner');
      await plugin.cancel();
    } catch {
      // Sem leitor aberto, não há o que cancelar.
    }
  }
}
