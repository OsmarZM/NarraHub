import { Injectable, computed, signal } from '@angular/core';

/**
 * Uma ação secundária da tela atual: renomear, tags, compartilhar, criar…
 *
 * No desktop essas ações são botões em linha no cabeçalho de cada área. No celular elas não cabem
 * lado a lado; a tela as PUBLICA aqui como dados, e o MobileShell as mostra no "•••" da barra de
 * cima, numa folha. O handler é o mesmo do botão do desktop — nenhuma lógica é duplicada.
 */
export interface ShellAction {
  id: string;
  label: string;
  icon?: string;
  disabled?: boolean;
  /** Ação ligada/desligada (ex.: resumo aberto). */
  active?: boolean;
  run: () => void;
}

interface Publicacao {
  owner: object;
  title: string;
  status: string;
  actions: ShellAction[];
}

@Injectable({ providedIn: 'root' })
export class ShellActionsState {
  private readonly publicacao = signal<Publicacao | null>(null);

  readonly actions = computed(() => this.publicacao()?.actions ?? []);
  readonly title = computed(() => this.publicacao()?.title ?? '');
  readonly status = computed(() => this.publicacao()?.status ?? '');

  /** Quem publica é dono das ações; publicar de novo substitui. */
  publish(owner: object, title: string, actions: ShellAction[], status = ''): void {
    this.publicacao.set({ owner, title, status, actions });
  }

  /** Só o dono limpa — uma tela que sai não apaga as ações da que acabou de entrar. */
  clear(owner: object): void {
    if (this.publicacao()?.owner === owner) this.publicacao.set(null);
  }
}
