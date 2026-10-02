import { Component, OnInit, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { LegacyRecoveryItem } from '../../core/native/legacy-recovery.service';
import { LegacyRecoveryStore } from './state/legacy-recovery.store';

/**
 * **Versões antigas para recuperar** (etapa H, H-R3).
 *
 * Para o escritor isto não é "Sync V1": é texto dele que ficou guardado só neste aparelho quando a
 * sincronização anterior foi desligada. Cada item tem duas saídas, e nenhuma delas é automática:
 * virar capítulo novo (e aí sincroniza como qualquer conteúdo) ou ser descartado, com confirmação.
 *
 * Substituir o capítulo atual não é oferecido: não existe causalidade para essa decisão histórica.
 */
@Component({
  selector: 'app-legacy-recovery-page',
  standalone: true,
  imports: [FormsModule, RouterLink],
  templateUrl: './legacy-recovery-page.component.html',
  styleUrl: './legacy-recovery-page.component.css',
})
export class LegacyRecoveryPageComponent implements OnInit {
  readonly store = inject(LegacyRecoveryStore);

  readonly livroEscolhido = signal('');
  readonly titulo = signal('');
  readonly confirmandoDescarte = signal(false);

  ngOnInit(): void {
    void this.store.load();
  }

  async abrir(item: LegacyRecoveryItem): Promise<void> {
    this.confirmandoDescarte.set(false);
    await this.store.open(item);
    const sugerido = this.store.destinations().find((destino) => destino.eOLivroOriginal)
      ?? this.store.destinations()[0];
    this.livroEscolhido.set(sugerido?.bookId ?? '');
    this.titulo.set(item.tituloAtual ? `${item.tituloAtual} — versão recuperada` : 'Capítulo recuperado');
  }

  fechar(): void {
    this.confirmandoDescarte.set(false);
    this.store.close();
  }

  preservar(): void {
    void this.store.preserve(this.livroEscolhido(), this.titulo());
  }

  descartar(): void {
    if (!this.confirmandoDescarte()) {
      this.confirmandoDescarte.set(true);
      return;
    }
    this.confirmandoDescarte.set(false);
    void this.store.discard();
  }

  rotuloDoEstado(status: string): string {
    switch (status) {
      case 'preserved': return 'Preservada';
      case 'discarded': return 'Descartada';
      default: return 'Precisa de decisão';
    }
  }

  /** O texto sem marcação, como o escritor o lê. */
  texto(valor: string): string {
    if (!valor) return '—';
    if (!valor.includes('<')) return valor;
    const limpo = valor
      .replace(/<\/(p|h[1-6]|li|blockquote|div)>/giu, '\n')
      .replace(/<br\s*\/?>/giu, '\n')
      .replace(/<[^>]+>/gu, '')
      .replace(/&nbsp;/gu, ' ')
      .replace(/&amp;/gu, '&')
      .replace(/&lt;/gu, '<')
      .replace(/&gt;/gu, '>')
      .trim();
    return limpo || '—';
  }
}
