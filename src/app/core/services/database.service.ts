import { Injectable } from '@angular/core';

interface SqlPool {
  close(db?: string): Promise<boolean>;
}

/**
 * Ciclo de vida do pool do `tauri-plugin-sql` — e **só** isso.
 *
 * Depois da Fase 4 este serviço não executa mais SQL: quem fala com o banco é
 * o core Rust, por comando. O plugin continua carregado por dois motivos, e
 * nenhum deles envolve consulta:
 *
 * 1. é ele que aplica as migrations na abertura do app;
 * 2. restaurar um backup precisa **fechar** o pool antes de trocar o arquivo
 *    do banco no disco, e reabri-lo depois.
 *
 * Por isso a capability perdeu `sql:allow-execute` e manteve `sql:default`.
 * Se algum dia voltar um `execute`/`select` aqui, a permissão precisa voltar
 * junto — há teste de fronteira cobrindo os dois lados dessa decisão.
 */
@Injectable({ providedIn: 'root' })
export class DatabaseService {
  private pool: SqlPool | null = null;
  private readonly path = 'sqlite:narrahub.db';
  private opening: Promise<void> | null = null;

  async init(): Promise<void> {
    if (this.pool) return;
    if (this.opening) return this.opening;
    this.opening = this.open();
    return this.opening;
  }

  private async open(): Promise<void> {
    try {
      const { default: Database } = await import('@tauri-apps/plugin-sql');
      this.pool = await (Database as unknown as { load(path: string): Promise<SqlPool> }).load(this.path);
      console.log('[NarraHub] Database connected');
    } catch (error) {
      console.error('[NarraHub] Database init failed:', error);
      this.opening = null;
      throw error;
    }
  }

  async close(): Promise<void> {
    if (this.opening) await this.opening.catch(() => undefined);
    if (!this.pool) {
      this.opening = null;
      return;
    }
    const closed = await this.pool.close();
    if (!closed) throw new Error('O pool SQLite recusou o encerramento necessário para restaurar o backup.');
    this.pool = null;
    this.opening = null;
    console.log('[NarraHub] Database connection pool closed');
  }
}
