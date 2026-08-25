// ============================================
// NarraHub — Database Service
// Wraps tauri-plugin-sql for Angular
// ============================================

import { Injectable } from '@angular/core';

// Types will be available at runtime via Tauri
declare const __TAURI__: any;

interface Database {
  execute(query: string, bindValues?: unknown[]): Promise<{ rowsAffected: number; lastInsertId: number }>;
  select<T = unknown>(query: string, bindValues?: unknown[]): Promise<T[]>;
  close(db?: string): Promise<boolean>;
}

@Injectable({ providedIn: 'root' })
export class DatabaseService {
  private db: Database | null = null;
  private dbPath = 'sqlite:narrahub.db';
  private initPromise: Promise<void> | null = null;

  async init(): Promise<void> {
    if (this.db) return;
    if (this.initPromise) return this.initPromise;

    this.initPromise = this._init();
    return this.initPromise;
  }

  private async _init(): Promise<void> {
    try {
      const { default: Database } = await import('@tauri-apps/plugin-sql');
      this.db = await (Database as any).load(this.dbPath);
      console.log('[NarraHub] Database connected');
    } catch (err) {
      console.error('[NarraHub] Database init failed:', err);
      throw err;
    }
  }

  async execute(query: string, params: unknown[] = []): Promise<{ rowsAffected: number; lastInsertId: number }> {
    await this.init();
    return this.db!.execute(query, params);
  }

  async select<T = unknown>(query: string, params: unknown[] = []): Promise<T[]> {
    await this.init();
    return this.db!.select<T>(query, params);
  }

  async selectOne<T = unknown>(query: string, params: unknown[] = []): Promise<T | null> {
    const results = await this.select<T>(query, params);
    return results.length > 0 ? results[0] : null;
  }

  async close(): Promise<void> {
    if (this.initPromise) await this.initPromise;
    if (!this.db) {
      this.initPromise = null;
      return;
    }
    const closed = await this.db.close();
    if (!closed) throw new Error('O pool SQLite recusou o encerramento necessário para restaurar o backup.');
    this.db = null;
    this.initPromise = null;
    console.log('[NarraHub] Database connection pool closed');
  }

  // Utility: generate UUID v4
  generateId(): string {
    return crypto.randomUUID();
  }

  // Utility: current ISO datetime
  now(): string {
    return new Date().toISOString().replace('T', ' ').substring(0, 19);
  }
}
