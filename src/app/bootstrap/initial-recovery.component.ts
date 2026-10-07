import { Component, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { BackupManifest, BackupService } from '../core/native/backup.service';
import { AppBootstrapService } from './app-bootstrap.service';

/** Recuperação antes de abrir o pool ou criar um banco novo. */
@Component({
  selector: 'app-initial-recovery',
  standalone: true,
  imports: [FormsModule],
  template: `
    <section class="initial-recovery surface-card" aria-labelledby="recovery-title">
      <span class="eyebrow">SEU ACERVO</span>
      <h1 id="recovery-title">Começar ou recuperar</h1>
      <p>Nenhum acervo foi encontrado neste perfil. Se você já usava o NarraHub e perdeu os dados do app, escolha seu arquivo de backup externo.</p>
      <div class="recovery-actions">
        <button type="button" class="primary-button" [disabled]="busy()" (click)="choose()">Abrir backup externo</button>
        <button type="button" class="secondary-button" [disabled]="busy()" (click)="bootstrap.createNewArchive()">Criar novo acervo</button>
      </div>
      @if (backup(); as selected) {
        <div class="recovery-preview">
          <h2>Backup validado</h2>
          <p>{{ selected.createdAt }} · NarraHub {{ selected.appVersion }} · schema {{ selected.schemaVersion }}</p>
          <p>{{ selected.assets.count }} arquivos de mídia · {{ selected.assets.totalBytes }} bytes</p>
          <p>O conteúdo será instalado neste perfil. A identidade do aparelho será criada pelo aplicativo; outros aparelhos precisarão ser pareados novamente.</p>
          <label>Digite RESTAURAR para confirmar<input [(ngModel)]="confirmation" autocomplete="off" [disabled]="busy()" /></label>
          <button type="button" class="primary-button" [disabled]="busy() || confirmation.trim() !== 'RESTAURAR'" (click)="restore()">Restaurar e reiniciar</button>
        </div>
      }
      @if (busy()) { <p role="status">Validando e preparando o acervo…</p> }
      @if (error()) { <p role="alert">{{ error() }}</p> }
      <small>O arquivo de backup deve estar em um local externo ao app. Excluir os dados do aplicativo também exclui seus snapshots internos.</small>
    </section>
  `,
  styles: `
    :host { display: block; padding: clamp(16px, 4vw, 48px); }
    .initial-recovery { max-width: 720px; margin: 0 auto; padding: clamp(20px, 4vw, 36px); overflow-wrap: anywhere; }
    h1 { margin: 12px 0; } p { line-height: 1.6; }
    .recovery-actions { display:flex; flex-wrap:wrap; gap:12px; margin:24px 0; }
    button { min-height:44px; white-space:normal; max-width:100%; }
    .recovery-preview { padding-block:20px; border-top:1px solid var(--line); }
    label { display:grid; gap:8px; margin-block:16px; }
    input { min-width:0; width:100%; box-sizing:border-box; min-height:44px; padding:12px; border:1px solid var(--line); border-radius:10px; color:var(--text); background:var(--surface-2); }
    small { display:block; line-height:1.6; }
  `,
})
export class InitialRecoveryComponent {
  readonly bootstrap = inject(AppBootstrapService);
  private readonly service = inject(BackupService);
  readonly busy = signal(false);
  readonly backup = signal<BackupManifest | null>(null);
  readonly error = signal('');
  confirmation = '';

  async choose(): Promise<void> {
    if (this.busy()) return;
    this.busy.set(true);
    this.error.set('');
    this.backup.set(null);
    this.confirmation = '';
    try { this.backup.set(await this.service.importExternal()); }
    catch (error) { this.error.set(error instanceof Error ? error.message : String(error)); }
    finally { this.busy.set(false); }
  }

  async restore(): Promise<void> {
    const backup = this.backup();
    if (!backup || this.busy() || this.confirmation.trim() !== 'RESTAURAR') return;
    this.busy.set(true);
    this.error.set('');
    try {
      const prepared = await this.service.prepareRestore(backup.backupId);
      await this.service.commitRestore(prepared.token);
      await this.service.restartAfterRestore();
    } catch (error) { this.error.set(error instanceof Error ? error.message : String(error)); }
    finally { this.busy.set(false); }
  }
}
