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
];

test('features extraídas não conhecem SQL nem o serviço legado', () => {
  for (const path of featureFiles) {
    const source = readFileSync(new URL(path, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /DatabaseService|WorkspaceService|UniverseService|EntityService|AttachmentService|\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE FROM\b/u, path);
  }
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
