import { Component, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { ConflictAction, ConflictSummary } from '../../core/native/sync-conflicts.service';
import { ConflictsStore } from './state/conflicts.store';

/**
 * A tela de conflitos do Sync V2 (etapa F): lista, detalhe com as duas versões, e a decisão.
 *
 * Página roteada sem `@Input()`: tudo vem do `ConflictsStore`.
 */
@Component({
  selector: 'app-conflicts-page',
  standalone: true,
  imports: [FormsModule, RouterLink],
  templateUrl: './conflicts-page.component.html',
  styleUrl: './conflicts-page.component.css',
})
export class ConflictsPageComponent implements OnInit {
  readonly store = inject(ConflictsStore);

  /** O nome digitado para a ação de renomear, por tag. */
  readonly novoNome = signal<Record<string, string>>({});

  readonly tipos = computed(() => this.opcoes((c) => [c.aggregateType, c.rotuloDoTipo]));
  readonly universos = computed(() => this.opcoes((c) => [c.universeId, c.universeId]));

  ngOnInit(): void {
    void this.store.load();
  }

  filtrar(campo: 'status' | 'kind' | 'aggregateType' | 'universeId', valor: string): void {
    void this.store.setFilter({ [campo]: valor });
  }

  async abrir(conflito: ConflictSummary): Promise<void> {
    await this.store.open(conflito.conflictKey);
    // No celular o detalhe fica abaixo da lista; sem isso, o toque parece não ter feito nada.
    requestAnimationFrame(() =>
      document.querySelector('[data-testid="conflito-detalhe"]')?.scrollIntoView({ block: 'start', behavior: 'smooth' }),
    );
  }

  nomeDe(tagId: string): string {
    return this.novoNome()[tagId] ?? '';
  }

  digitarNome(tagId: string, valor: string): void {
    this.novoNome.update((atual) => ({ ...atual, [tagId]: valor }));
  }

  decidir(acao: ConflictAction): void {
    if (acao.pedeNome && !this.nomeDe(acao.tagId).trim()) return;
    void this.store.resolve(acao.acao, acao.tagId, this.nomeDe(acao.tagId).trim());
  }

  rotuloDoKind(kind: string): string {
    switch (kind) {
      case 'parent_deletion_blocked': return 'Exclusão bloqueada';
      case 'tag_name_conflict': return 'Tag com o mesmo nome';
      default: return 'Versões diferentes';
    }
  }

  /** Um valor de campo para leitura humana. */
  mostrar(valor: unknown): string {
    if (valor === null || valor === undefined || valor === '') return '—';
    if (typeof valor === 'string') return this.textoSimples(valor);
    if (typeof valor === 'number' || typeof valor === 'boolean') return String(valor);
    if (Array.isArray(valor) && valor.length === 0) return '—';
    return JSON.stringify(valor, null, 1);
  }

  private textoSimples(valor: string): string {
    if (!valor.includes('<')) return valor;
    const texto = valor
      .replace(/<\/(p|h[1-6]|li|blockquote|div)>/giu, '\n')
      .replace(/<br\s*\/?>/giu, '\n')
      .replace(/<[^>]+>/gu, '')
      .replace(/&nbsp;/gu, ' ')
      .replace(/&amp;/gu, '&')
      .replace(/&lt;/gu, '<')
      .replace(/&gt;/gu, '>')
      .trim();
    return texto || '—';
  }

  private opcoes(par: (c: ConflictSummary) => [string, string]): Array<{ valor: string; rotulo: string }> {
    const vistos = new Map<string, string>();
    for (const conflito of this.store.conflicts()) {
      const [valor, rotulo] = par(conflito);
      if (valor && !vistos.has(valor)) vistos.set(valor, rotulo);
    }
    return [...vistos].map(([valor, rotulo]) => ({ valor, rotulo }));
  }
}
