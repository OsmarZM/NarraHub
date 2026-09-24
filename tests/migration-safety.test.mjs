import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const ler = (relativo) => readFileSync(new URL(relativo, import.meta.url), 'utf8');

test('o plugin SQL não migra no setup: migration só depois do portão e do backup', () => {
  // Com `preload`, o tauri-plugin-sql aplicava as migrations no setup do plugin — antes do portão
  // de compatibilidade e sem backup — e um banco mais novo derrubava o app com VersionMissing.
  const config = JSON.parse(ler('../src-tauri/tauri.conf.json'));
  assert.deepEqual(config.plugins?.sql?.preload ?? [], [], 'preload do plugin SQL voltou');
});

test('arranque: compatibilidade → backup → migração → confirmação, e rollback em qualquer falha', () => {
  const boot = ler('../src/app/bootstrap/app-bootstrap.service.ts');
  const inicializacao = boot.slice(boot.indexOf('private async runInitialization'), boot.indexOf('private async openDatabaseSafely'));
  const compat = inicializacao.indexOf('this.backupService.compatibility()');
  const abrir = inicializacao.indexOf('await this.openDatabaseSafely()');
  assert.ok(compat >= 0 && abrir > compat, 'o banco abre depois do portão de compatibilidade');
  assert.doesNotMatch(inicializacao, /await this\.db\.init\(\)/u, 'db.init direto pula o backup');

  const seguro = boot.slice(boot.indexOf('private async openDatabaseSafely'));
  const preparar = seguro.indexOf('this.backupService.prepareMigration()');
  const init = seguro.indexOf('await this.db.init()');
  const concluir = seguro.indexOf('this.backupService.finishMigration()');
  const desfazer = seguro.indexOf('this.backupService.rollbackMigration()');
  assert.ok(preparar >= 0 && init > preparar, 'o backup vem antes de o plugin migrar');
  assert.ok(concluir > init, 'a migration só é dada como concluída depois de aplicada');
  assert.ok(desfazer > concluir, 'falha em migrar ou em confirmar devolve o banco original');
  assert.match(seguro, /catch \(error\) \{[\s\S]*rollbackMigration/u);
});

test('os três comandos da migration segura estão registrados', () => {
  const lib = ler('../src-tauri/src/lib.rs');
  for (const comando of ['database_migration_prepare', 'database_migration_finish', 'database_migration_rollback']) {
    assert.ok(lib.includes(`database::upgrade::${comando}`), `${comando} fora do invoke_handler`);
  }
});

// I-BUG-02 (Etapa I): o merge de configuração do Tauri SUBSTITUI arrays. Um perfil que declara
// `app.windows` só para trocar o título apaga decorations/maximized/tamanhos da janela base — e a
// janela abre com a barra do Windows por cima da do app e estreita demais para as ações do topo.
test('perfis que sobrescrevem a janela repetem a entrada base inteira, mudando só o título', () => {
  const base = JSON.parse(ler('../src-tauri/tauri.conf.json')).app.windows;
  for (const perfil of ['tauri.qualification.conf.json', 'tauri.production.conf.json']) {
    const janelas = JSON.parse(ler(`../src-tauri/${perfil}`)).app?.windows;
    if (!janelas) continue;
    assert.equal(janelas.length, base.length, perfil);
    janelas.forEach((janela, i) => {
      const { title: _t1, ...resto } = janela;
      const { title: _t2, ...restoBase } = base[i];
      assert.deepEqual(resto, restoBase, `${perfil}: janela ${i} difere da base além do título`);
    });
  }
});
