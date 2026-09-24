import { Component, OnInit, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import {
  ConflictAction,
  ConflictDetail,
  ConflictDiffLine,
  ConflictField,
  ConflictSide,
  ConflictSummary,
} from '../../core/native/sync-conflicts.service';
import { ShellState } from '../../shell/state/shell.state';
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
  private readonly router = inject(Router);
  private readonly shell = inject(ShellState);

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

  /**
   * "Escolher e editar" (I-BUG-05, pedido do operador na Etapa I): a resolução continua sendo escolher uma
   * das versões — o formato congelado não carrega texto novo —, e a edição depois é um evento comum.
   */
  permiteEditar(detalhe: ConflictDetail, acao: ConflictAction): boolean {
    return detalhe.resumo.aggregateType === 'chapter'
      && ['ficarComA', 'ficarComB', 'manterLocal', 'restaurar'].includes(acao.acao);
  }

  async decidirEEditar(acao: ConflictAction): Promise<void> {
    const resumo = this.store.selected()?.resumo;
    if (!resumo) return;
    if (!await this.store.resolve(acao.acao, acao.tagId, '')) return;
    await this.router.navigate(['/workspace', resumo.universeId, 'writing', resumo.aggregateId]);
  }

  async copiar(lado: ConflictSide): Promise<void> {
    try {
      await navigator.clipboard.writeText(this.textoDe(lado));
      this.shell.showInfo(`Texto ${lado.desteAparelho ? 'deste aparelho' : 'do outro aparelho'} copiado.`);
    } catch {
      this.shell.showError('Não foi possível copiar. Selecione o texto e copie manualmente.');
    }
  }

  /** O texto do capítulo daquele lado, sem HTML. Vazio quando o item não tem texto. */
  textoDe(lado: ConflictSide): string {
    const campo = lado.campos.find((c) => c.campo === 'content');
    const texto = typeof campo?.valor === 'string' ? this.textoSimples(campo.valor) : '';
    return texto === '—' ? '' : texto;
  }

  /** Parágrafos do texto de um lado, marcando os que não existem do outro lado. */
  paragrafos(detalhe: ConflictDetail, lado: ConflictSide): Array<{ texto: string; difere: boolean }> {
    const outro = lado === detalhe.desteAparelho ? detalhe.doOutro : detalhe.desteAparelho;
    const doOutro = new Set(this.partes(this.textoDe(outro)));
    return this.partes(this.textoDe(lado)).map((texto) => ({ texto, difere: !doOutro.has(texto) }));
  }

  /** As linhas que mudaram, com as iguais colapsadas em "… N parágrafos iguais …". */
  linhasQueMudaram(detalhe: ConflictDetail): ConflictDiffLine[] {
    const saida: ConflictDiffLine[] = [];
    let iguais = 0;
    const fechar = () => {
      if (!iguais) return;
      saida.push({ tipo: 'igual', texto: `… ${iguais} ${iguais === 1 ? 'parágrafo igual' : 'parágrafos iguais'} …` });
      iguais = 0;
    };
    for (const linha of detalhe.diffDeTexto) {
      if (linha.tipo === 'igual') { iguais += 1; continue; }
      fechar();
      saida.push(linha);
    }
    if (saida.length) fechar();
    return saida;
  }

  /** Campos daquele lado que diferem do outro — fora o texto (mostrado acima) e os identificadores. */
  camposQueDiferem(detalhe: ConflictDetail, lado: ConflictSide): ConflictField[] {
    const diferentes = new Set(detalhe.diferencas.map((d) => d.campo));
    return lado.campos.filter((c) => this.visivel(c) && diferentes.has(c.campo));
  }

  camposIguais(detalhe: ConflictDetail, lado: ConflictSide): ConflictField[] {
    const diferentes = new Set(detalhe.diferencas.map((d) => d.campo));
    return lado.campos.filter((c) => this.visivel(c) && !diferentes.has(c.campo));
  }

  diferencasForaDoTexto(detalhe: ConflictDetail): ConflictDetail['diferencas'] {
    return detalhe.diferencas.filter((d) => d.campo !== 'content' && !this.eIdentificador(d.campo));
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

  private visivel(campo: ConflictField): boolean {
    return campo.campo !== 'content' && !this.eIdentificador(campo.campo);
  }

  /** `id`, `bookId`, `universeId`…: referência interna, não informação para o escritor. */
  private eIdentificador(campo: string): boolean {
    return campo === 'id' || /Id$/u.test(campo);
  }

  private partes(texto: string): string[] {
    return texto.split('\n').map((p) => p.trim()).filter(Boolean);
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
