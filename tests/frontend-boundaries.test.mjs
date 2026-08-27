import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const featureFiles = [
  '../src/app/features/timeline/timeline-page.component.ts',
  '../src/app/features/timeline/state/timeline.store.ts',
  '../src/app/features/timeline/gateways/timeline.gateway.ts',
  '../src/app/features/history/history-page.component.ts',
  '../src/app/features/history/state/history.store.ts',
  '../src/app/features/history/gateways/history.gateway.ts',
  '../src/app/features/library/library-page.component.ts',
  '../src/app/features/library/state/universe.store.ts',
  '../src/app/features/library/gateways/universe.gateway.ts',
  '../src/app/features/entities/entities-page/entities-page.component.ts',
  '../src/app/features/entities/entity-sheet/entity-sheet.component.ts',
  '../src/app/features/entities/components/entity-card/entity-card.component.ts',
  '../src/app/features/entities/components/entity-toolbar/entity-toolbar.component.ts',
  '../src/app/features/entities/components/entity-type-filter/entity-type-filter.component.ts',
  '../src/app/features/entities/state/entity.store.ts',
  '../src/app/features/entities/gateways/entity.gateway.ts',
  '../src/app/features/settings/settings-page.component.ts',
  '../src/app/features/collaboration/state/collaboration.store.ts',
  '../src/app/features/collaboration/gateways/collaboration.gateway.ts',
  '../src/app/features/collaboration/share-modal/share-modal.component.ts',
];

test('features extraídas não conhecem SQL nem o serviço legado', () => {
  for (const path of featureFiles) {
    const source = readFileSync(new URL(path, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /DatabaseService|WorkspaceService|UniverseService|EntityService|AttachmentService|CollaborationService|\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE FROM\b/u, path);
  }
});

test('SettingsStore não usa SQL nem serviços legados de outro domínio, só DatabaseService para o pool', () => {
  // Diferente dos outros stores, o SettingsStore não tem um LegacyXGateway: backup,
  // sync e update já são comandos Tauri nativos, sem SQL por trás. A única exceção
  // sancionada é DatabaseService, usado só para fechar/reabrir o pool SQLite durante
  // uma restauração de backup — não é o limite SQL-vs-Rust que os outros gateways
  // abstraem, é gerência de ciclo de vida da conexão.
  const source = readFileSync(new URL('../src/app/features/settings/state/settings.store.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /WorkspaceService|UniverseService|EntityService|AttachmentService|\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE FROM\b/u);
  assert.match(source, /DatabaseService/u, 'a exceção sancionada deve continuar explícita e comentada no arquivo');
});

test('dependência SQL temporária fica restrita aos adapters legados', () => {
  for (const feature of ['timeline', 'history']) {
    const source = readFileSync(new URL(`../src/app/features/${feature}/gateways/legacy-${feature}.gateway.ts`, import.meta.url), 'utf8');
    assert.match(source, /WorkspaceService/u);
  }
  const universeSource = readFileSync(new URL('../src/app/features/library/gateways/legacy-universe.gateway.ts', import.meta.url), 'utf8');
  assert.match(universeSource, /UniverseService/u);
  const entitySource = readFileSync(new URL('../src/app/features/entities/gateways/legacy-entity.gateway.ts', import.meta.url), 'utf8');
  assert.match(entitySource, /EntityService/u);
  assert.match(entitySource, /AttachmentService/u);
  const collaborationSource = readFileSync(new URL('../src/app/features/collaboration/gateways/legacy-collaboration.gateway.ts', import.meta.url), 'utf8');
  assert.match(collaborationSource, /CollaborationService/u);
});

test('App raiz não acessa mais o serviço legado de universos diretamente', () => {
  const source = readFileSync(new URL('../src/app/app.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /UniverseService/u, 'app.ts deve depender de UniverseStore/UniverseGateway, não do serviço legado');
});

test('App raiz delega o domínio de entidades para a feature', () => {
  const source = readFileSync(new URL('../src/app/app.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/app.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /EntityService|AttachmentService|newEntityName|entityGallery|entityAiBusy/u);
  assert.doesNotMatch(template, /activeEntity\(|newEntityName|entityGallery\(|patchActiveEntity|updateActiveEntity/u);
  assert.match(template, /<app-entities-page/u);
});

test('App raiz delega configurações, backup, IA local, sync e atualização para a feature', () => {
  const source = readFileSync(new URL('../src/app/app.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/app.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /BackupService|SyncService|UpdateService|ThemeService|aiInstallBusy|aiSelectedProfile/u);
  assert.doesNotMatch(template, /settingsSection\(|theme\.preference\(|backupBusy\(|syncStatus\(/u);
  assert.match(template, /<app-settings-page/u);
});

test('App raiz delega colaboração e compartilhamento para a feature', () => {
  const source = readFileSync(new URL('../src/app/app.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/app.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /CollaborationService|\bOnlineShareService\b|shareSelectedUniverseIds|rememberShare/u);
  assert.doesNotMatch(template, /isUniverseSelectedForShare|selectAllShareUniverses|shareChoice/u);
  assert.match(template, /<app-share-modal/u);
});
