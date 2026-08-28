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
  '../src/app/features/planning/planning-board.component.ts',
  '../src/app/features/planning/state/planning.store.ts',
  '../src/app/features/planning/gateways/planning.gateway.ts',
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
  const planningSource = readFileSync(new URL('../src/app/features/planning/gateways/legacy-planning.gateway.ts', import.meta.url), 'utf8');
  assert.match(planningSource, /PlanningService/u);
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

test('Timeline migra para rota lazy própria e passa a injetar EntityStore/KnowledgeStore direto (Fase 3.2)', () => {
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const workspaceSource = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const workspaceTemplate = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  const timeline = readFileSync(new URL('../src/app/features/timeline/timeline-page.component.ts', import.meta.url), 'utf8');
  assert.match(routes, /path: 'timeline'[\s\S]*?loadComponent: \(\) => import\('\.\/features\/timeline\/timeline-page\.component'\)/u);
  assert.doesNotMatch(workspaceSource, /TimelinePageComponent/u, 'WorkspaceLayout não deve mais importar/declarar TimelinePageComponent — ele chega pelo router-outlet');
  assert.doesNotMatch(workspaceTemplate, /<app-timeline-page/u, 'a árvore legacy não deve montar uma segunda instância da página de timeline');
  // Timeline não pode mais receber entities/tagsByOwner por @Input() de um pai de template — o pai agora é o
  // Router, não o WorkspaceLayout — então passa a injetar os stores cross-domain diretamente, como
  // ManuscriptStore/ConnectionsStore já fazem em suas próprias páginas roteadas/quase-roteadas.
  assert.doesNotMatch(timeline, /@Input\(\) entities|@Input\(\) tagsByOwner|@Output\(\).*metadataRequested/u);
  assert.match(timeline, /inject\(EntityStore\)/u);
  assert.match(timeline, /inject\(KnowledgeStore\)/u);
  // O gatilho de "+ Evento" no cabeçalho persistente não alcança mais a página roteada por @ViewChild;
  // o (activate) do <router-outlet> precisa entregar a instância ativa para isso continuar funcionando.
  assert.match(workspaceTemplate, /\(activate\)="onWorkspaceOutletActivate\(\$event\)"/u);
  assert.match(workspaceSource, /supportsCreate\(this\.activeRoutedPage\)/u);
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

test('o <router-outlet> do workspace fica DENTRO de .workspace-view (regressão: página roteada renderizada fora da área visível)', () => {
  // Bug real reportado como "Histórico parou de funcionar / tela em branco". A página roteada
  // ESTAVA no DOM (por isso extrair texto da página a encontrava), mas o outlet era irmão logo
  // DEPOIS de <section class="workspace-view">, que é height:100% dentro de um
  // .workspace-route-content overflow:hidden. Medido no navegador: .workspace-route-content
  // top=64/bottom=720, section.workspace-view ocupando os mesmos 656px, e app-history-page
  // começando em top=720 — ou seja, 100% abaixo da borda visível e cortada pelo overflow.
  // O outlet precisa ocupar o mesmo slot de conteúdo que as páginas legacy.
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  const sectionStart = template.indexOf('<section class="workspace-view');
  const sectionEnd = template.indexOf('</section>', sectionStart);
  const outlet = template.indexOf('<router-outlet');
  assert.ok(sectionStart >= 0 && sectionEnd > sectionStart, 'section.workspace-view deve existir');
  assert.ok(outlet > sectionStart && outlet < sectionEnd,
    'o <router-outlet> precisa estar dentro de <section class="workspace-view">, não depois dela — fora, a página roteada renderiza abaixo da área visível e some atrás do overflow:hidden');
});



test('restoreRoute() não usa activeNav() pra detectar navegação já processada (regressão do bug de deep link em Histórico/Timeline)', () => {
  // Bug real: activeNav() é computed(() => navigation.activeData().navigationId), ou seja, é derivado
  // direto de route.data e já reflete a rota nova assim que o Router resolve — ANTES de
  // selectNav()/returnToLibrary()/openSettings() rodarem e atualizarem AppState.workspaceView(). Comparar
  // `route.navId === activeNav()` em restoreRoute() sempre dava "já processado" e a chamada a selectNav()
  // nunca acontecia depois de um deep link ou reload direto pra uma seção roteada (History, Timeline),
  // deixando appState.workspaceView() preso na view antiga (ex.: 'editor') por baixo do <router-outlet>.
  // Reproduzido manualmente via ng serve com um universo fake seedado direto no UniverseStore: navegar
  // (via URL, sem clicar na sidebar) direto pra /workspace/:id/history mostrava o conteúdo de Escrita E
  // de Histórico ao mesmo tempo.
  const source = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(
    source,
    /if \(route\.navId === this\.activeNav\(\)\)|route\.navId === this\.activeNav\(\)/u,
    'restoreRoute() não pode voltar a comparar route.navId com activeNav() — activeNav() é derivado da própria rota e sempre "bate", mascarando quando o workspaceView() de fato precisa ser sincronizado',
  );
  assert.match(source, /private lastSyncedRouteKey/u);
  assert.match(source, /private routeKey\(/u);
  // selectNav/returnToLibrary/openSettings/setWorkspaceNavigation precisam marcar a chave ANTES do
  // trabalho assíncrono, senão o effect que dispara restoreRoute() (via navigation.route()) pode competir
  // com a própria selectNav() em andamento.
  assert.match(source, /this\.lastSyncedRouteKey = this\.routeKey\(item\.id, targetUniverseId\)/u);
  assert.match(source, /this\.lastSyncedRouteKey = this\.routeKey\('inicio', null\)/u);
  assert.match(source, /this\.lastSyncedRouteKey = this\.routeKey\('configuracoes', null\)/u);
  assert.match(source, /this\.lastSyncedRouteKey = this\.routeKey\(navId, this\.appState\.activeUniverseId\(\)\)/u);
});

test('canvas de Conexões não vira uma segunda fonte de relações canônicas', () => {
  // As duas coisas convivem de propósito: `relations` guarda FATOS do universo
  // (aparecem na ficha da entidade, FK obrigando entidade nas duas pontas) e
  // `canvas_edges` guarda anotação de diagrama (pontas polimórficas). Trocar uma
  // pela outra corromperia o significado do cânone, então a migration v14 não
  // pode ter tocado em `relations`.
  const migrations = readFileSync(new URL('../src-tauri/src/database/migrations.rs', import.meta.url), 'utf8');
  const v14 = migrations.slice(migrations.indexOf('pub const MIGRATION_V14'), migrations.indexOf('#[cfg(test)]'));
  assert.doesNotMatch(v14, /ALTER TABLE relations|DROP TABLE relations|CREATE TABLE IF NOT EXISTS relations/u,
    'a migration do canvas não pode alterar nem reconstruir a tabela de relações canônicas');
  for (const table of ['canvas_nodes', 'canvas_entity_positions', 'canvas_edges']) {
    assert.match(v14, new RegExp(`CREATE TABLE IF NOT EXISTS ${table}`, 'u'));
  }
});

test('a feature de Conexões não conhece SQL nem DatabaseService', () => {
  // CanvasService é o adapter legado (fala SQL); a feature só pode vê-lo através
  // do gateway, igual aos outros domínios extraídos.
  for (const path of [
    '../src/app/features/connections/connections-page.component.ts',
    '../src/app/features/connections/connections-graph.component.ts',
    '../src/app/features/connections/state/connections.store.ts',
    '../src/app/features/connections/gateways/connections.gateway.ts',
  ]) {
    const source = readFileSync(new URL(path, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /DatabaseService|CanvasService|WorkspaceService|\bSELECT\b|\bINSERT\b|\bDELETE FROM\b/u, path);
  }
  const legacy = readFileSync(new URL('../src/app/features/connections/gateways/legacy-connections.gateway.ts', import.meta.url), 'utf8');
  assert.match(legacy, /CanvasService/u, 'o adapter legado é quem pode conhecer o serviço SQL do canvas');
});

test('toda migration existente esta registrada no tauri-plugin-sql (regressao: canvas v14 nunca rodava no app)', () => {
  // Bug real: MIGRATION_V14 existia em migrations.rs e no match de sql_for_version
  // (usado por recovery e pelos testes Rust), mas nao foi adicionada a lista do
  // tauri-plugin-sql em lib.rs -- que e o que de fato aplica migrations no app
  // rodando. Resultado: as tabelas do canvas nunca eram criadas, todo INSERT
  // falhava e "adicionar elemento" nao fazia nada. Os testes Rust passavam porque
  // chamam sql_for_version direto, contornando o registro.
  const migrations = readFileSync(new URL('../src-tauri/src/database/migrations.rs', import.meta.url), 'utf8');
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');

  const declared = [...migrations.matchAll(/^pub const MIGRATION_V(\d+):/gmu)].map((m) => Number(m[1])).sort((a, b) => a - b);
  const latest = Number(/LATEST_SCHEMA_VERSION: i64 = (\d+)/u.exec(migrations)[1]);
  const registered = [...lib.matchAll(/version: (\d+),\s*description:/gu)].map((m) => Number(m[1])).sort((a, b) => a - b);

  assert.deepEqual(registered, declared,
    `toda MIGRATION_Vn precisa estar em add_migrations() de lib.rs. Declaradas: ${declared}. Registradas: ${registered}.`);
  assert.equal(latest, Math.max(...declared),
    'LATEST_SCHEMA_VERSION precisa acompanhar a ultima migration declarada');
});

test('Fase 3 concluída: toda seção do workspace é rota lazy e o layout não hospeda página', () => {
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const layout = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  const template = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');

  // toda seção carrega por loadComponent, nenhuma sobrou como children: []
  for (const section of ['writing', 'entities', 'connections', 'timeline', 'planning', 'history']) {
    const at = routes.indexOf(`path: '${section}'`);
    assert.ok(at >= 0, `a rota ${section} precisa existir`);
    assert.ok(routes.slice(at, at + 320).includes('loadComponent'),
      `a seção ${section} precisa ser uma rota lazy (loadComponent)`);
  }
  assert.ok(!routes.includes('children: []'), 'nenhuma seção pode continuar sem componente roteado');

  // o layout virou moldura: nenhuma página no template nem nos imports
  for (const tag of ['app-writing-page', 'app-entities-page', 'app-connections-page', 'app-planning-board', 'app-timeline-page', 'app-history-page']) {
    assert.doesNotMatch(template, new RegExp(`<${tag}`, 'u'), `${tag} não pode ser montada pelo layout`);
  }
  assert.doesNotMatch(layout, /WritingPageComponent|EntitiesPageComponent|ConnectionsPageComponent|PlanningBoardComponent|TimelinePageComponent|HistoryPageComponent/u);
  assert.match(template, /<router-outlet/u);

  // o gatilho de criação do cabeçalho chega pelo outlet, não por @ViewChild de página
  for (const page of ['Writing', 'Entities', 'Connections', 'Planning', 'Timeline', 'History']) {
    assert.ok(!layout.includes(`@ViewChild(${page}`), `@ViewChild(${page}...) não alcança página vinda do outlet`);
  }
  assert.match(layout, /beginCreateOnActivePage/u);
});

test('Fase 3 concluída: AppState não representa mais a rota ativa', () => {
  const appState = readFileSync(new URL('../src/app/core/state/app.state.ts', import.meta.url), 'utf8');
  // Só o código conta: o comentário que explica a remoção pode citar o nome.
  const code = appState.split(String.fromCharCode(10)).filter((line) => !line.trim().startsWith('*') && !line.trim().startsWith('//')).join(' ');
  assert.ok(!code.includes('workspaceView') && !code.includes('WorkspaceView'),
    'workspaceView duplicava a URL e já causou tela em branco quando os dois discordaram');
  for (const method of ['openEditor', 'openEntityList', 'openEntitySheet', 'openGraph', 'openTimeline', 'openPlanning', 'openHistory', 'openSettings']) {
    assert.ok(!appState.includes(`${method}(`), `${method}() só existia para escolher a tela — isso é papel do Router`);
  }
  // varredura geral: nada no app pode voltar a decidir tela por workspaceView
  const layout = readFileSync(new URL('../src/app/workspace-layout.component.html', import.meta.url), 'utf8');
  assert.ok(!layout.includes('appState.workspaceView'));
});

test('coordenação cross-domain vive acima das features, não num layout', () => {
  // Regra da Fase 3: desmontar o layout não pode transferir coordenação de
  // domínio para outro componente monolítico.
  const sync = readFileSync(new URL('../src/app/application/workspace-sync.service.ts', import.meta.url), 'utf8');
  assert.match(sync, /onEntityMutated/u);
  assert.match(sync, /onChapterPersisted/u);
  const layout = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(layout, /onEntityMutation|onChapterPersisted/u,
    'o layout não pode voltar a orquestrar efeitos entre domínios');
});

test('sair de Escrita salva o capítulo, sem substituir o saveNow() explícito', () => {
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const guard = readFileSync(new URL('../src/app/routing/unsaved-chapter.guard.ts', import.meta.url), 'utf8');
  assert.ok(routes.includes('canDeactivate: [unsavedChapterGuard]'));
  assert.ok(guard.includes('saveNow()'));
  assert.match(guard, /return true/u, 'o guard nunca bloqueia: prender o usuário na tela não recupera o texto');
  // fechar janela / atualizar / restaurar backup acontecem fora do Router
  const root = readFileSync(new URL('../src/app/root-layout.component.ts', import.meta.url), 'utf8');
  assert.ok(root.includes('manuscript.saveNow()'), 'fechar a janela continua salvando por conta própria');
});

test('página roteada não declara @Input que a rota não alimenta (regressão: input zerado por withComponentInputBinding)', () => {
  // withComponentInputBinding() SOBRESCREVE com undefined todo @Input sem
  // correspondente em params/data da rota — o inicializador da classe não
  // protege. Isso quebrou o template de Conexões com "reading 'length' of
  // undefined" quando a página deixou de receber [entities] do layout.
  // Só `universeId` vem da rota (via paramsInheritanceStrategy: 'always').
  const pages = {
    'manuscript/writing-page': 'writing-page',
    'entities/entities-page/entities-page': 'entities-page',
    'connections/connections-page': 'connections-page',
    'planning/planning-board': 'planning-board',
    'timeline/timeline-page': 'timeline-page',
    'history/history-page': 'history-page',
  };
  for (const path of Object.keys(pages)) {
    const source = readFileSync(new URL(`../src/app/features/${path}.component.ts`, import.meta.url), 'utf8');
    const inputs = [...source.matchAll(/@Input\([^)]*\)\s+(\w+)/gu)].map((m) => m[1]);
    assert.deepEqual(inputs, ['universeId'],
      `${path} só pode declarar @Input() universeId — o resto vem de store. Encontrado: ${inputs.join(', ') || '(nenhum)'}`);
  }
});
