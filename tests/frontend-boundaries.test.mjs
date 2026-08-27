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
  '../src/app/features/manuscript/writing-page.component.ts',
  '../src/app/features/manuscript/state/manuscript.store.ts',
  '../src/app/features/manuscript/gateways/manuscript.gateway.ts',
  '../src/app/features/connections/connections-page.component.ts',
  '../src/app/features/connections/state/connections.store.ts',
  '../src/app/features/connections/gateways/connections.gateway.ts',
  '../src/app/features/knowledge/state/knowledge.store.ts',
  '../src/app/features/knowledge/gateways/knowledge.gateway.ts',
  '../src/app/features/knowledge/tags-modal/tags-modal.component.ts',
];

test('features extraídas não conhecem SQL nem o serviço legado', () => {
  for (const path of featureFiles) {
    const source = readFileSync(new URL(path, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /DatabaseService|WorkspaceService|UniverseService|EntityService|AttachmentService|CollaborationService|StoryService|BookService|ChapterService|MentionService|MetadataService|\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE FROM\b/u, path);
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
  const manuscriptSource = readFileSync(new URL('../src/app/features/manuscript/gateways/legacy-manuscript.gateway.ts', import.meta.url), 'utf8');
  assert.match(manuscriptSource, /StoryService/u);
  assert.match(manuscriptSource, /BookService/u);
  assert.match(manuscriptSource, /ChapterService/u);
  const connectionsSource = readFileSync(new URL('../src/app/features/connections/gateways/legacy-connections.gateway.ts', import.meta.url), 'utf8');
  assert.match(connectionsSource, /WorkspaceService/u);
  const knowledgeSource = readFileSync(new URL('../src/app/features/knowledge/gateways/legacy-knowledge.gateway.ts', import.meta.url), 'utf8');
  assert.match(knowledgeSource, /MetadataService/u);
  assert.match(knowledgeSource, /MentionService/u);
});

test('App raiz é somente o outlet do Router', () => {
  const source = readFileSync(new URL('../src/app/app.ts', import.meta.url), 'utf8');
  assert.match(source, /RouterOutlet/u);
  assert.doesNotMatch(source, /features\/|AppState|DatabaseService|SettingsStore/u);
});

test('bootstrap global termina antes do Router e não depende do RootLayout', () => {
  const config = readFileSync(new URL('../src/app/app.config.ts', import.meta.url), 'utf8');
  const bootstrap = readFileSync(new URL('../src/app/bootstrap/app-bootstrap.service.ts', import.meta.url), 'utf8');
  const layout = readFileSync(new URL('../src/app/root-layout.component.ts', import.meta.url), 'utf8');
  assert.match(config, /provideAppInitializer\(\(\) => inject\(AppBootstrapService\)\.initialize\(\)\)/u);
  assert.match(config, /\.\/bootstrap\/app-bootstrap\.service/u);
  assert.match(bootstrap, /await this\.db\.init\(\)/u);
  assert.doesNotMatch(bootstrap, /RootLayoutComponent|ActivatedRoute|Router/u);
  assert.doesNotMatch(layout, /ngOnInit|this\.db\.init\(\)|this\.ai\.initialize\(\)/u);
});

test('resolver seleciona somente o universo depois do bootstrap e falha de forma recuperável', () => {
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const resolver = readFileSync(new URL('../src/app/routing/universe.resolver.ts', import.meta.url), 'utf8');
  assert.match(routes, /resolve: \{ universe: universeResolver \}/u);
  assert.match(resolver, /bootstrap\.error\(\)/u);
  assert.match(resolver, /universes\.universes\(\)\.find/u);
  assert.match(resolver, /appState\.openUniverse\(universe\)/u);
  assert.match(resolver, /router\.parseUrl\('\/library'\)/u);
  assert.doesNotMatch(resolver, /EntityStore|ManuscriptStore|TimelineStore|HistoryStore|PlanningService|ConnectionsStore|KnowledgeStore/u);
});

test('sidebar e navegação ativa derivam de route.data', () => {
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const navigation = readFileSync(new URL('../src/app/core/navigation/app-navigation.service.ts', import.meta.url), 'utf8');
  const workspace = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  assert.match(routes, /navigationData\('historico', 'Histórico'/u);
  assert.doesNotMatch(routes, /path: ':section'/u);
  assert.match(navigation, /current\.data/u);
  assert.match(navigation, /collectNavigationItems\(this\.router\.config\)/u);
  assert.match(workspace, /this\.navigation\.activeData\(\)\.navigationId/u);
  assert.match(workspace, /this\.navigation\.navigationItems\.map/u);
  assert.doesNotMatch(workspace, /activeNav\.set/u);
  assert.match(template, /libraryBreadcrumbLabel/u);
});

test('RootLayout não importa páginas de domínio nem serviços legados', () => {
  const source = readFileSync(new URL('../src/app/root-layout.component.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /UniverseService/u, 'RootLayout deve depender de UniverseStore/UniverseGateway, não do serviço legado');
  assert.doesNotMatch(source, /LibraryPageComponent|WritingPageComponent|EntitiesPageComponent|ConnectionsPageComponent|TimelinePageComponent|PlanningBoardComponent|HistoryPageComponent/u);
});

test('WorkspaceLayout mantém uma única árvore legacy e delega entidades para a feature', () => {
  const source = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /EntityService|AttachmentService|newEntityName|entityGallery|entityAiBusy/u);
  assert.doesNotMatch(template, /activeEntity\(|newEntityName|entityGallery\(|patchActiveEntity|updateActiveEntity/u);
  assert.match(template, /<app-entities-page/u);
  assert.equal((template.match(/<app-entities-page/gu) || []).length, 1);
});

test('Histórico é a primeira seção do workspace migrada para rota lazy própria (Fase 3.2)', () => {
  const config = readFileSync(new URL('../src/app/app.config.ts', import.meta.url), 'utf8');
  assert.match(config, /withComponentInputBinding\(\)/u, 'universeId da rota precisa chegar como @Input() sem binding manual');
  assert.match(config, /paramsInheritanceStrategy: 'always'/u, 'a rota filha de história precisa herdar :universeId da rota pai do workspace');
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const workspaceSource = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const workspaceTemplate = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  const history = readFileSync(new URL('../src/app/features/history/history-page.component.ts', import.meta.url), 'utf8');
  assert.match(routes, /path: 'history'[\s\S]*?loadComponent: \(\) => import\('\.\/features\/history\/history-page\.component'\)/u);
  assert.doesNotMatch(workspaceSource, /HistoryPageComponent/u, 'WorkspaceLayout não deve mais importar/declarar HistoryPageComponent — ele chega pelo router-outlet');
  assert.doesNotMatch(workspaceTemplate, /<app-history-page/u, 'a árvore legacy não deve montar uma segunda instância da página de histórico');
  assert.match(workspaceTemplate, /<router-outlet/u);
  assert.match(history, /@Input\(\{ required: true \}\) universeId/u, 'segue recebendo universeId por Input — agora vindo de withComponentInputBinding()/paramsInheritanceStrategy, não de um binding manual do layout');
});

test('Settings é rota global lazy e não depende de universo ativo', () => {
  const source = readFileSync(new URL('../src/app/root-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/root-layout.component.html', import.meta.url), 'utf8');
  const settings = readFileSync(new URL('../src/app/features/settings/settings-page.component.ts', import.meta.url), 'utf8');
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /BackupService|SyncService|UpdateService|ThemeService|aiInstallBusy|aiSelectedProfile/u);
  assert.doesNotMatch(template, /settingsSection\(|theme\.preference\(|backupBusy\(|syncStatus\(/u);
  assert.match(template, /<router-outlet/u);
  assert.match(routes, /features\/settings\/settings-page\.component/u);
  assert.match(routes, /features\/library\/library-route\.component/u);
  assert.match(routes, /workspace-layout\.component/u);
  assert.doesNotMatch(settings, /activeUniverseId|AppState/u);
});

test('WorkspaceLayout delega colaboração e compartilhamento para a feature', () => {
  const source = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /CollaborationService|\bOnlineShareService\b|shareSelectedUniverseIds|rememberShare/u);
  assert.doesNotMatch(template, /isUniverseSelectedForShare|selectAllShareUniverses|shareChoice/u);
  assert.match(template, /<app-share-modal/u);
});

test('WorkspaceLayout delega história/livro/capítulo e o editor para a feature de Manuscrito', () => {
  const source = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /StoryService|BookService|ChapterService|newStoryName|newChapterTitle|editorContent\(|applyFormat/u);
  assert.doesNotMatch(template, /selectTreeChapter\(|chapter-editor|activeEntityId/u);
  assert.match(template, /<app-writing-page/u);
});

test('WorkspaceLayout delega conexões (relações) para a feature', () => {
  const source = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /WorkspaceService|newRelationSource|newRelationTarget|newRelationLabel|loadRelations|createRelation\b/u);
  assert.doesNotMatch(template, /app-connections-graph|relation-pills-list|new-relation/u);
  assert.match(template, /<app-connections-page/u);
});

test('WorkspaceLayout delega tags e menções (Knowledge) para a feature', () => {
  const source = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /MetadataService|MentionService|metadataTarget|metadataTags|newTagName|newTagColor|groupTagAssignments|contentParagraphs|textMentionsEntity/u);
  assert.doesNotMatch(template, /metadata-tag-item|metadata-new-tag/u);
  assert.match(template, /<app-tags-modal/u);
});

test('rotas de biblioteca e workspace são lazy e não coexistem no RootLayout', () => {
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const root = readFileSync(new URL('../src/app/root-layout.component.ts', import.meta.url), 'utf8');
  const rootTemplate = readFileSync(new URL('../src/app/root-layout.component.html', import.meta.url), 'utf8');
  assert.match(routes, /loadComponent: \(\) => import\('\.\/workspace-layout\.component'\)/u);
  assert.match(routes, /loadComponent: \(\) => import\('\.\/features\/library\/library-route\.component'\)/u);
  assert.doesNotMatch(root, /WorkspaceLayoutComponent|LibraryRouteComponent/u);
  assert.doesNotMatch(rootTemplate, /app-writing-page|app-entities-page|app-library-page/u);
  assert.match(rootTemplate, /<router-outlet/u);
});
