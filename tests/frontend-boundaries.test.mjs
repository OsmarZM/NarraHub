import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
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

test('nenhum arquivo do frontend executa SQL (critério de saída da Fase 4)', () => {
  // O inverso do teste que existia aqui: ele conferia que o SQL do frontend
  // ficava confinado aos adaptadores legados. A Fase 4 terminou, esses
  // adaptadores não existem mais, e o critério agora é que ninguém no
  // frontend escreva SQL — é isso que justifica ter tirado `sql:allow-execute`
  // da capability. Se este teste falhar, a permissão precisa voltar antes.
  const offenders = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) { walk(path); continue; }
      if (!entry.name.endsWith('.ts')) continue;
      const source = readFileSync(path, 'utf8');
      if (/\bFROM\s+\w+|\bINSERT\s+(OR\s+\w+\s+)?INTO\b|\bUPDATE\s+\w+\s+SET\b|\bDELETE\s+FROM\b/u.test(source)) {
        offenders.push(path);
      }
    }
  };
  // `src/` inteiro, e não só `src/app/`: um arquivo com SQL fora da pasta de código do
  // Angular continuaria invisível para este teste, que é exatamente como o andaime
  // superado da pasta acima do repositório poderia voltar sem ninguém ver.
  walk(fileURLToPath(new URL('../src/', import.meta.url)));
  assert.deepEqual(offenders, [], `SQL no frontend: ${offenders.join(', ')}`);
});

test('o andaime superado não volta para o repositório', () => {
  // Fora deste repositório, na pasta que o contém, existe um andaime de 2026-08-20 que o
  // NarraHub atual substituiu por inteiro: `angular-src/` com 66 ocorrências de SQL em
  // serviços que hoje são proibidos, `rust-src/commands/` byte a byte igual ao diretório
  // removido na Fase 3, e `design-system/` anterior ao `styles.css`.
  //
  // Ele não é versionado de propósito — decisão registrada em `docs/ai/PROJECT_STATE.md`.
  // Este teste existe porque a forma mais provável de ele voltar não é alguém decidir
  // reintroduzi-lo: é alguém copiar a pasta para dentro do repositório sem perceber o que
  // ela contém. Num projeto onde três agentes fazem `grep`, código morto com nome vivo é
  // pior que código morto apagado.
  const proibidos = ['angular-src', 'rust-src', 'design-system'];
  const raiz = fileURLToPath(new URL('../', import.meta.url));
  for (const nome of proibidos) {
    assert.ok(
      !existsSync(join(raiz, nome)),
      `${nome}/ apareceu na raiz do repositório. É o andaime anterior ao NarraHub atual: `
        + 'contém SQL no frontend e comandos legados que os gates desta suíte existem para '
        + 'manter fora. Se o objetivo era preservar histórico, o Git já tem o do código real.',
    );
  }
});

test('a camada de serviços SQL legada foi removida', () => {
  const gone = [
    'core/services/universe.service.ts',
    'core/services/story.service.ts',
    'core/services/book.service.ts',
    'core/services/chapter.service.ts',
    'core/services/entity.service.ts',
    'core/services/mention.service.ts',
    'core/services/metadata.service.ts',
    'core/services/workspace.service.ts',
    'core/services/canvas.service.ts',
    'core/services/attachment.service.ts',
    'core/services/collaboration.service.ts',
    'core/services/planning.service.ts',
  ];
  for (const relative of gone) {
    assert.ok(
      !existsSync(fileURLToPath(new URL(`../src/app/${relative}`, import.meta.url))),
      `${relative} voltou — o core Rust é quem fala com o banco agora`,
    );
  }

  const legacy = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.startsWith('legacy-') && entry.name.endsWith('.gateway.ts')) legacy.push(path);
    }
  };
  walk(fileURLToPath(new URL('../src/app/features/', import.meta.url)));
  assert.deepEqual(legacy, [], `adaptador legado remanescente: ${legacy.join(', ')}`);
});

test('DatabaseService sobrou apenas para o ciclo de vida do pool', () => {
  // A exceção sancionada: fechar e reabrir o pool na restauração de backup, e
  // abri-lo no bootstrap para o tauri-plugin-sql aplicar as migrations. Nenhum
  // desses caminhos executa SQL — se `execute`/`select` reaparecerem, a
  // dependência voltou a ser de dados.
  const source = readFileSync(new URL('../src/app/core/services/database.service.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /\bexecute\s*\(|\bselect\s*</u,
    'DatabaseService não pode mais expor execução de SQL');
  assert.match(source, /close\s*\(/u, 'o fechamento do pool continua sendo necessário para restaurar backup');
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

test('GATE DA FASE 2: as quatro proibições do WorkspaceLayout', () => {
  // O roadmap define o gate desta fase como quatro proibições, e não como contagem de
  // linhas: "linhas são consequência, não arquitetura". Este teste é a versão executável
  // delas. Os testes acima cobrem cada extração; este cobre a regra que as motivou.
  const layout = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');

  // 1. Não conhecer implementação de gateway.
  assert.doesNotMatch(layout, /Gateway\b/u,
    'o layout fala com stores e serviços de aplicação, nunca com gateway');

  // 2. Não montar payload de compartilhamento.
  assert.doesNotMatch(layout, /SharedUniverse|OnlineShareDocument/u,
    'montar o documento compartilhado é do WorkspaceShareService');

  // 3. Não carregar domínios manualmente.
  for (const store of ['entityStore', 'timelineStore', 'manuscriptStore', 'planningStore', 'knowledgeStore']) {
    assert.ok(!layout.includes(`${store}.load(`),
      `carregar ${store} é do WorkspaceSessionService`);
  }

  // 4. Não executar regra de domínio. SQL nunca; e as regras que atravessam domínios
  //    pertencem ao WorkspaceSyncService.
  assert.doesNotMatch(layout, /\bSELECT\b|\bINSERT\b|\bUPDATE\b|\bDELETE FROM\b/u,
    'o layout não executa SQL');
  assert.doesNotMatch(layout, /refreshAfterExternalChange|rebuildMentionIndex|syncChapterMentions/u,
    'coordenação entre domínios é do WorkspaceSyncService');
});

test('WorkspaceLayout não é dono do ciclo de vida da sessão de universo (Fase 2)', () => {
  // O layout acumulava navegação, lifecycle, preload, busca, sharing, backup e updates. O
  // ciclo de vida da sessão saiu primeiro porque é o que as outras extrações dependem.
  //
  // O que este teste protege não é o tamanho do arquivo — é o layout não voltar a saber
  // QUAIS stores existem e em que ordem zerá-los. Toda vez que um domínio novo entrar, quem
  // precisa saber disso é o WorkspaceSessionService, num lugar só.
  const layout = readFileSync(new URL('../src/app/workspace-layout.component.ts', import.meta.url), 'utf8');

  assert.doesNotMatch(layout, /\w+Store\.reset\(\)/u,
    'zerar stores é do WorkspaceSessionService: o layout não pode conhecer a lista de domínios');
  assert.doesNotMatch(layout, /narrahub\.lastUniverseId/u,
    'a persistência do último universo aberto pertence ao WorkspaceSessionService');
  // `universeStore.load()` continua aqui de propósito: é a lista da biblioteca, não a
  // pré-carga de um universo aberto. O que não pode voltar são os cinco domínios do workspace.
  for (const store of ['entityStore', 'timelineStore', 'manuscriptStore', 'planningStore', 'knowledgeStore']) {
    assert.ok(!layout.includes(`${store}.load(`),
      `a pré-carga de ${store} saiu do layout e é do WorkspaceSessionService`);
  }
  assert.doesNotMatch(layout, /rebuildMentionIndex/u,
    'reconstruir o índice de menções é parte da sessão, não do layout');
  assert.match(layout, /inject\(WorkspaceSessionService\)/u,
    'o layout delega a sessão em vez de reimplementá-la');

  // Compartilhamento: o layout orquestra a sessão e avisa o usuário, mas não sabe montar o
  // documento nem comprimir imagem. Enquanto isso morava aqui, mudar o formato do documento
  // compartilhado passava pelo arquivo mais movimentado do frontend.
  assert.ok(!layout.includes('toDataURL'),
    'compressão de imagem é do WorkspaceShareService, não de um componente de layout');
  assert.ok(!layout.includes("createElement('canvas')"),
    'o layout não manipula canvas para preparar imagem de compartilhamento');
  assert.doesNotMatch(layout, /SharedUniverse/u,
    'montar o universo compartilhado é do WorkspaceShareService');
  assert.match(layout, /inject\(WorkspaceShareService\)/u,
    'o layout delega a montagem do documento compartilhado');

  const share = readFileSync(new URL('../src/app/application/workspace-share.service.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(share, /shell\.|showInfo|showError/u,
    'o serviço de compartilhamento monta o documento; quem avisa o usuário é quem o chamou');

  // Coordenação cross-domain: aprovar uma revisão de colaboração mexe em manuscrito,
  // entidades e estatísticas. O layout dispara o evento; quem sabe a sequência é o
  // WorkspaceSyncService — senão toda página que aprove revisão teria que repeti-la.
  assert.doesNotMatch(layout, /refreshAfterExternalChange/u,
    'reler domínio após mudança externa é coordenação cross-domain, não do layout');
  assert.doesNotMatch(layout, /refreshStats\(/u,
    'recalcular estatísticas do universo é do WorkspaceSyncService');
  assert.match(layout, /inject\(WorkspaceSyncService\)/u,
    'o layout dispara o evento e delega a sequência');

  // E o serviço não pode puxar interface para dentro: ele existe para ser chamado por
  // qualquer caminho, não só pelo layout.
  const session = readFileSync(new URL('../src/app/application/workspace-session.service.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(session, /@angular\/router|Router\b/u,
    'quem navega é o layout; a sessão só cuida dos dados');
  assert.doesNotMatch(session, /\bSELECT\b|\bINSERT\b|\bUPDATE\b/u,
    'a sessão fala com stores, nunca com SQL');
});

test('invariante 8: a IA não tem por onde escrever conteúdo canônico', () => {
  // Diferente das outras dez, esta invariante não é provada por um teste no core Rust.
  // Ela é garantida pela AUSÊNCIA de caminho de escrita: o AiService não injeta store nem
  // gateway nenhum, então não existe como uma resposta de IA virar conteúdo salvo sem que
  // uma página peça isso explicitamente.
  //
  // O dia em que alguém injetar um store aqui — por conveniência, para "já salvar" — a
  // garantia deixa de existir sem que nenhum outro teste perceba. Ver
  // docs/DOMAIN_INVARIANTS.md.
  const source = readFileSync(new URL('../src/app/core/native/ai.service.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(source, /inject\(\s*\w*(Store|Gateway|DatabaseService)\s*\)/u,
    'o AiService não pode injetar store, gateway nem DatabaseService: é a ausência desse caminho que garante que a IA não altera conteúdo canônico sem confirmação');
  assert.doesNotMatch(source, /\b(INSERT|UPDATE|DELETE)\b/u,
    'o AiService não pode conter SQL');
});

test('o pool só é aberto depois do portão de compatibilidade de schema (ADR 0007)', () => {
  // O incidente de 2026-09-01: instalar uma versão antiga sobre um banco novo fazia o app
  // não abrir, sem janela e sem mensagem, porque a falha acontece dentro do
  // provideAppInitializer -- antes de a interface existir para mostrar o erro.
  // Se esta ordem se inverter, o app volta a morrer em silêncio.
  const bootstrap = readFileSync(new URL('../src/app/bootstrap/app-bootstrap.service.ts', import.meta.url), 'utf8');
  const portao = bootstrap.indexOf('this.backupService.compatibility()');
  const abertura = bootstrap.indexOf('this.db.init()');
  assert.notEqual(portao, -1, 'o bootstrap precisa consultar a compatibilidade do schema');
  assert.notEqual(abertura, -1, 'o bootstrap continua responsável por abrir o pool');
  assert.ok(portao < abertura, 'a verificação de compatibilidade tem que vir ANTES de abrir o pool');
  assert.match(bootstrap, /if \(!compatibility\.compatible\)[\s\S]{0,160}return;/u,
    'banco incompatível precisa interromper a inicialização sem abrir o pool');

  // A tela de recuperação não pode depender do banco: ela existe justamente para o caso
  // em que o banco não pode ser aberto.
  const recovery = readFileSync(new URL('../src/app/bootstrap/schema-recovery.component.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(recovery, /inject\(DatabaseService\)/u,
    'a tela de recuperação não pode injetar o DatabaseService: ela existe justamente para quando o banco não abre');

  // E o layout precisa dar a ela o lugar todo: nenhuma rota funciona sem o pool.
  const layout = readFileSync(new URL('../src/app/root-layout.component.html', import.meta.url), 'utf8');
  assert.match(layout, /schemaIncompatible\(\)[\s\S]*<app-schema-recovery/u);
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
  // O canvas fala com o core Rust; a feature só o alcança pelo gateway, igual
  // aos outros domínios. `CanvasService` não existe mais.
  for (const path of [
    '../src/app/features/connections/connections-page.component.ts',
    '../src/app/features/connections/connections-graph.component.ts',
    '../src/app/features/connections/state/connections.store.ts',
    '../src/app/features/connections/gateways/connections.gateway.ts',
  ]) {
    const source = readFileSync(new URL(path, import.meta.url), 'utf8');
    assert.doesNotMatch(source, /DatabaseService|CanvasService|WorkspaceService|\bSELECT\b|\bINSERT\b|\bDELETE FROM\b/u, path);
  }
  const rust = readFileSync(new URL('../src/app/features/connections/gateways/rust-connections.gateway.ts', import.meta.url), 'utf8');
  assert.match(rust, /RustCoreService/u, 'o adaptador Rust é a única porta da feature para o banco');
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
  //
  // O permitido não é uma lista fixa: é `universeId` (herdado por
  // paramsInheritanceStrategy: 'always') mais os params que a PRÓPRIA rota da
  // página declara. Deriva-se de app.routes.ts para o teste não precisar ser
  // reescrito a cada deep link novo — e para pegar o inverso, que é o erro
  // caro: input declarado sem param correspondente.
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const allowedByPage = new Map();
  for (const match of routes.matchAll(/path: '([^']+)'[^]*?loadComponent: \(\) => import\('\.\/features\/([^']+)\.component'/gu)) {
    const [, routePath, page] = match;
    const params = [...routePath.matchAll(/:(\w+)/gu)].map((param) => param[1]);
    const allowed = allowedByPage.get(page) ?? new Set(['universeId']);
    for (const param of params) allowed.add(param);
    allowedByPage.set(page, allowed);
  }

  const pages = [
    'manuscript/writing-page',
    'entities/entities-page/entities-page',
    'connections/connections-page',
    'planning/planning-board',
    'timeline/timeline-page',
    'history/history-page',
  ];
  for (const page of pages) {
    const allowed = allowedByPage.get(page);
    assert.ok(allowed, `${page} deveria estar em app.routes.ts via loadComponent`);
    const source = readFileSync(new URL(`../src/app/features/${page}.component.ts`, import.meta.url), 'utf8');
    const inputs = [...source.matchAll(/@Input\([^)]*\)\s+(\w+)/gu)].map((m) => m[1]);
    assert.ok(inputs.includes('universeId'), `${page} precisa declarar @Input() universeId`);
    const extra = inputs.filter((input) => !allowed.has(input));
    assert.deepEqual(extra, [],
      `${page} declara @Input que nenhuma rota alimenta (viram undefined em runtime): ${extra.join(', ')}. Permitidos: ${[...allowed].join(', ')}`);
  }
});

test('store de universo deduplica carga (regressão: layout e página carregavam o mesmo SQL duas vezes)', () => {
  // O layout pré-carrega os cinco domínios para a busca global e, logo em
  // seguida, o ngOnChanges da página roteada chama load() de novo. Sem guarda
  // isso é o dobro de SQL em toda entrada de seção. Quem quer recarregar de
  // verdade (abrir universo, refresh após mutação) passa `force: true`.
  const stores = [
    'manuscript/state/manuscript.store',
    'entities/state/entity.store',
    'knowledge/state/knowledge.store',
    'planning/state/planning.store',
    'connections/state/connections.store',
    'timeline/state/timeline.store',
  ];
  for (const store of stores) {
    const source = readFileSync(new URL(`../src/app/features/${store}.ts`, import.meta.url), 'utf8');
    assert.match(source, /async load\(universeId: string, force = false\)/u,
      `${store}.load precisa aceitar force para diferenciar pré-carga de refresh`);
    assert.match(source, /if \(!force && this\.\w+ === universeId\) return;/u,
      `${store}.load precisa sair cedo quando o universo já está carregado`);
  }
});

test('rota de deep link não duplica o item da sidebar', () => {
  // Angular não tem parâmetro opcional: /writing e /writing/:chapterId são
  // duas entradas. As duas carregam o mesmo navigationId, então sem
  // hiddenFromMenu a sidebar mostraria "Escrita" duas vezes — que foi
  // exatamente a queixa que originou esta regra.
  const routes = readFileSync(new URL('../src/app/app.routes.ts', import.meta.url), 'utf8');
  const starts = [...routes.matchAll(/path: '([^']+)'/gu)];
  for (let i = 0; i < starts.length; i++) {
    const path = starts[i][1];
    if (!path.includes(':')) continue;
    const block = routes.slice(starts[i].index, starts[i + 1]?.index ?? routes.length);
    if (!block.includes('navigationData(')) continue;
    assert.ok(block.includes('hiddenFromMenu: true'),
      `a rota '${path}' declara navigationData sem hiddenFromMenu — vai duplicar o item na sidebar`);
  }

  const service = readFileSync(new URL('../src/app/core/navigation/app-navigation.service.ts', import.meta.url), 'utf8');
  assert.match(service, /!route\.data\.hiddenFromMenu/u,
    'collectNavigationItems precisa pular as rotas marcadas como hiddenFromMenu');
});

test('a capability não concede mais execução de SQL ao frontend', () => {
  // Critério de saída da Fase 4: nenhum componente depende do SQL do
  // frontend, então `sql:allow-execute` saiu. `sql:default` fica, porque o
  // plugin ainda aplica as migrations e o pool precisa ser fechado para
  // restaurar backup.
  const capability = readFileSync(
    new URL('../src-tauri/capabilities/default.json', import.meta.url), 'utf8',
  );
  const permissions = JSON.parse(capability).permissions.map((item) =>
    (typeof item === 'string' ? item : item.identifier));

  assert.ok(!permissions.includes('sql:allow-execute'),
    'sql:allow-execute só pode voltar se o frontend voltar a executar SQL');
  assert.ok(permissions.includes('sql:default'),
    'o plugin ainda aplica as migrations e fecha o pool na restauração de backup');
});
