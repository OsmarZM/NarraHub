import { Injectable } from '@angular/core';
import { isTauri } from '@tauri-apps/api/core';

/**
 * Porta para a janela nativa: minimizar, maximizar, fechar, tela cheia e tema do sistema.
 *
 * Existe porque `getCurrentWindow()` estava espalhado por quatro arquivos — o serviço de
 * tema, os dois layouts e a página de escrita. Cada um importava a API do Tauri e mexia na
 * janela por conta própria.
 *
 * A distinção que justifica esta porta é a mesma que separa `core/native` de
 * `RustCoreService`: **isto é capacidade da plataforma, não domínio do produto.** Uma janela
 * não guarda o livro de ninguém; ela é do sistema operacional. Misturar as duas coisas na
 * mesma abstração foi o que fez a documentação afirmar que só existia uma porta Tauri,
 * enquanto o código tinha sete.
 *
 * Cada método é silencioso fora do desktop: no navegador não há janela nativa, e um
 * componente não deveria precisar saber disso para chamar `minimize()`.
 */
@Injectable({ providedIn: 'root' })
export class NativeWindowService {
  readonly available = isTauri();

  async minimize(): Promise<void> {
    await this.comJanela((win) => win.minimize());
  }

  async toggleMaximize(): Promise<void> {
    await this.comJanela((win) => win.toggleMaximize());
  }

  async close(): Promise<void> {
    await this.comJanela((win) => win.close());
  }

  /** Alterna tela cheia. Devolve o estado final, ou `false` fora do desktop. */
  async toggleFullscreen(): Promise<boolean> {
    let resultado = false;
    await this.comJanela(async (win) => {
      const alvo = !(await win.isFullscreen());
      await win.setFullscreen(alvo);
      resultado = alvo;
    });
    return resultado;
  }

  /**
   * Informa ao sistema o tema da janela, para que a moldura nativa acompanhe a interface.
   * `null` devolve a decisão ao sistema operacional.
   */
  async setTheme(theme: 'light' | 'dark' | null): Promise<void> {
    await this.comJanela((win) => win.setTheme(theme));
  }

  /**
   * Import dinâmico, e não estático: manter `@tauri-apps/api/window` fora do grafo inicial
   * evita que o bundle do navegador carregue código que só existe no desktop.
   */
  private async comJanela(acao: (win: Awaited<ReturnType<typeof this.janela>>) => unknown): Promise<void> {
    if (!this.available) return;
    const win = await this.janela();
    if (!win) return;
    await acao(win);
  }

  private async janela() {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow();
  }
}
