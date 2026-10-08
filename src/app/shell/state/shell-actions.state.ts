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

/**
 * Duas camadas, na ordem em que aparecem na folha:
 *
 *   page   ações da tela aberta (Conexões: nova conexão, nota, imagem…)
 *   area   ações da área que hospeda a tela (o universo: renomear, tags, compartilhar)
 */
export type ShellActionLayer = 'page' | 'area';

interface Publicacao {
  owner: object;
  title: string;
  status: string;
  actions: ShellAction[];
}

@Injectable({ providedIn: 'root' })
export class ShellActionsState {
  private readonly camadas = signal<Record<ShellActionLayer, Publicacao | null>>({ page: null, area: null });

  readonly actions = computed(() => [...(this.camadas().page?.actions ?? []), ...(this.camadas().area?.actions ?? [])]);
  readonly title = computed(() => this.camadas().area?.title || this.camadas().page?.title || '');
  readonly status = computed(() => this.camadas().area?.status || this.camadas().page?.status || '');

  /** Quem publica é dono das ações da sua camada; publicar de novo substitui. */
  publish(owner: object, title: string, actions: ShellAction[], status = '', layer: ShellActionLayer = 'area'): void {
    this.camadas.update((atual) => ({ ...atual, [layer]: { owner, title, status, actions } }));
  }

  /** Só o dono limpa — uma tela que sai não apaga as ações da que acabou de entrar. */
  clear(owner: object): void {
    this.camadas.update((atual) => ({
      page: atual.page?.owner === owner ? null : atual.page,
      area: atual.area?.owner === owner ? null : atual.area,
    }));
  }
}
