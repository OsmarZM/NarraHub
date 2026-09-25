import { ChangeDetectionStrategy, Component, Input } from '@angular/core';
import { renderSVG } from 'uqr';

/**
 * Desenha o QR de pareamento a partir do conteúdo opaco que o Rust montou (NH-084, PR B).
 *
 * Só apresentação: não lê, não valida e não monta o formato — isso é do Rust. O conteúdo carrega o
 * PIN, então nada aqui o registra; a imagem existe só enquanto a escuta tem código válido.
 */
@Component({
  selector: 'app-pairing-qr',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<img class="pairing-qr-image" [src]="imagem" alt="QR para parear: aponte a câmera do outro aparelho" width="200" height="200" data-testid="pairing-qr" />`,
  styles: [`:host { display: flex; justify-content: center; } .pairing-qr-image { width: 200px; height: 200px; padding: 10px; border-radius: 12px; background: #fff; image-rendering: pixelated; }`],
})
export class PairingQrComponent {
  imagem = '';

  @Input({ required: true }) set conteudo(valor: string) {
    const svg = renderSVG(valor, { ecc: 'M', border: 1, whiteColor: '#ffffff', blackColor: '#000000' });
    this.imagem = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  }
}
