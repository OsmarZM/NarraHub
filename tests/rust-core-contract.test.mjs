import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const featuresDir = fileURLToPath(new URL('../src/app/features/', import.meta.url));
const rustSrcDir = fileURLToPath(new URL('../src-tauri/src/', import.meta.url));
const libSource = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');

/** Todos os `rust-<dominio>.gateway.ts` e o contrato abstrato correspondente. */
function rustGateways() {
  const found = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.startsWith('rust-') && entry.name.endsWith('.gateway.ts')) found.push(path);
    }
  };
  walk(featuresDir);
  return found;
}

/** Nomes de método declarados num `abstract class X { abstract m(...) }`. */
function abstractMethods(source) {
  return [...source.matchAll(/^\s*abstract\s+(\w+)\s*\(/gmu)].map((match) => match[1]);
}

/** Nomes de método implementados numa classe concreta. */
function implementedMethods(source) {
  const body = source.slice(source.indexOf('export class'));
  return [...body.matchAll(/^\s{2}(?:async\s+)?(\w+)\s*\([^)]*\)\s*:/gmu)].map((match) => match[1]);
}

test('todo adaptador Rust implementa o contrato inteiro do gateway', () => {
  // O adaptador Rust convive com o legado durante a Fase 4: método ainda não
  // migrado delega. O risco é sumir com um método na troca — o TypeScript
  // pega quando a classe declara `implements`, mas não pega se alguém
  // afrouxar isso, e o sintoma seria "não é uma função" só em runtime.
  const gateways = rustGateways();
  assert.ok(gateways.length > 0, 'nenhum adaptador Rust encontrado — o teste ficaria vazio');

  for (const path of gateways) {
    const source = readFileSync(path, 'utf8');
    const contractPath = path.replace(/rust-([\w-]+)\.gateway\.ts$/u, '$1.gateway.ts');
    const contract = readFileSync(contractPath, 'utf8');

    const required = abstractMethods(contract);
    assert.ok(required.length > 0, `${contractPath} não declara nenhum método abstrato`);

    const implemented = new Set(implementedMethods(source));
    const missing = required.filter((method) => !implemented.has(method));
    assert.deepEqual(missing, [], `${path} não implementa: ${missing.join(', ')}`);
  }
});

test('todo comando chamado pelo frontend existe no Rust e está registrado no invoke_handler', () => {
  // Registrar o comando é um passo separado de escrevê-lo, e esquecê-lo
  // compila normalmente nos dois lados: o erro só aparece em runtime, no
  // clique do usuário. Foi exatamente assim que a migration v14 do canvas
  // passou meses sem nunca rodar.
  // Varre o core inteiro, e não só `interface/tauri`: `planning_save_card`
  // existe desde antes da Fase 4 e mora em `database/planning.rs`. Restringir
  // a busca faria o teste reprovar um comando que funciona.
  const declared = new Set();
  const walkRust = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) { walkRust(path); continue; }
      if (!entry.name.endsWith('.rs')) continue;
      const source = readFileSync(path, 'utf8');
      for (const match of source.matchAll(/#\[tauri::command\]\s*pub fn (\w+)/gu)) declared.add(match[1]);
    }
  };
  walkRust(rustSrcDir);
  assert.ok(declared.size > 0, 'nenhum comando #[tauri::command] encontrado no core');

  const called = new Set();
  for (const path of rustGateways()) {
    const source = readFileSync(path, 'utf8');
    for (const match of source.matchAll(/core\.call<[^>]*>\('(\w+)'/gu)) called.add(match[1]);
  }
  assert.ok(called.size > 0, 'nenhuma chamada ao core encontrada — o teste ficaria vazio');

  // O invoke_handler lista caminhos completos; basta o comando aparecer como
  // último segmento de uma das entradas.
  const registered = new Set(
    [...libSource.matchAll(/^\s+(?:[\w:]+::)?(\w+),$/gmu)].map((match) => match[1]),
  );

  for (const command of called) {
    assert.ok(declared.has(command), `o frontend chama '${command}', que não existe no core Rust`);
    assert.ok(
      registered.has(command),
      `'${command}' existe mas não está no invoke_handler de lib.rs — falharia só no clique do usuário`,
    );
  }
});

test('só as portas nativas falam com o Tauri', () => {
  // Este teste substitui um que prometia mais do que entregava: ele se chamava "o core Rust
  // não é chamado por fora do adaptador de gateway", mas procurava apenas a string
  // `RustCoreService` dentro de `features/`. Uma chamada direta a `invoke('sync_start')` num
  // componente passava sem ser vista.
  //
  // E a regra que ele tentava proteger também estava errada. A documentação dizia que
  // `RustCoreService` era a única porta Tauri do aplicativo, mas o produto real tem duas
  // coisas diferentes atravessando a mesma fronteira:
  //
  //   persistência de domínio   →  RustCoreService  →  interface/tauri
  //   capacidades da plataforma →  core/native/*    →  sync, share, IA, backup, updater
  //
  // Forçar as duas na mesma abstração produzia uma documentação que o código contradizia.
  // A regra verdadeira é mais simples e mais forte: componentes, stores, layouts e serviços
  // de aplicação nunca falam com o Tauri. Só as portas falam.
  const PORTAS_PERMITIDAS = [
    // A porta do núcleo de domínio.
    'core/services/rust-core.service.ts',
    // As portas de plataforma. Cada uma existe porque a capacidade é do sistema, não do
    // domínio: elas não gravam conteúdo do escritor, elas acionam o dispositivo.
    'core/native/sync.service.ts',
    'core/native/online-share.service.ts',
    'core/native/ai.service.ts',
    'core/native/backup.service.ts',
    'core/native/update.service.ts',
    'core/native/production-replica.service.ts',
    // A janela é do sistema operacional, não do produto: ela não guarda o livro
    // de ninguém. Antes desta porta, `getCurrentWindow()` estava em quatro arquivos.
    'core/native/window.service.ts',
    // Bytes de asset atravessando o IPC (ADR 0010). É plataforma, não domínio: o
    // que ela faz é publicar bytes e devolver algo renderizável, sem nunca
    // expor caminho de arquivo. O domínio guarda o hash, e o hash é portátil.
    'core/native/blob.service.ts',
    // Ciclo de vida do pool SQLite: abre e fecha a conexão, não executa SQL.
    'core/services/database.service.ts',
    // O estado do Sync V2 (etapa 14, fatia 2). É plataforma: o que ela lê é sobre
    // ESTE APARELHO -- identidade, roster, cursor, pendência --, e não conteúdo do
    // escritor. Distinta de `sync.service.ts`, que é a porta do V1 e está congelada.
    'core/native/sync-v2.service.ts',
    // Atualização do Android por APK das GitHub Releases. É plataforma: baixa e abre o instalador do
    // sistema, e não recebe URL nem caminho da tela -- o Rust usa o que ele próprio verificou.
    'core/native/android-update.service.ts',
    // Os conflitos do Sync V2 (etapa F). É plataforma pelo mesmo motivo do Sync V2: o que ela
    // lê é sobre a sincronização DESTE APARELHO. Ela recebe DTOs prontos -- nunca tabela de
    // sincronização, envelope nem revisão crua -- e a decisão viaja como ação portátil.
    'core/native/sync-conflicts.service.ts',
  ];

  const infratores = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const caminho = join(dir, entry.name);
      if (entry.isDirectory()) { walk(caminho); continue; }
      if (!entry.name.endsWith('.ts')) continue;
      const fonte = readFileSync(caminho, 'utf8');
      // `isTauri()` fica de fora de propósito: é detecção de ambiente, não capacidade.
      // Um componente precisa saber se está no desktop para decidir o que mostrar, e
      // proibir isso empurraria uma pergunta trivial para dentro de uma porta.
      //
      // O que a regra alcança é **acionar** o sistema: comando Tauri, janela, plugin.
      const falaComTauri = /\binvoke\s*[<(]/u.test(fonte)
        || fonte.includes("from '@tauri-apps/api/window'")
        || fonte.includes("from '@tauri-apps/plugin-")
        || /@tauri-apps\/api\/(path|event|shell|fs|dpi)/u.test(fonte);
      if (!falaComTauri) continue;
      const relativo = caminho.replace(/\\/gu, '/');
      if (PORTAS_PERMITIDAS.some((porta) => relativo.endsWith(porta))) continue;
      infratores.push(relativo.slice(relativo.indexOf('src/app/')));
    }
  };
  walk(fileURLToPath(new URL('../src/app/', import.meta.url)));

  assert.deepEqual(
    infratores,
    [],
    'só as portas de core/native e o RustCoreService podem falar com o Tauri. '
      + `Fora da lista: ${infratores.join(', ')}. `
      + 'Se a capacidade é nova, crie uma porta em core/native e acrescente-a à lista — '
      + 'com a justificativa de por que ela é plataforma e não domínio.',
  );
});

test('a porta de domínio não vira porta de plataforma', () => {
  // A separação só vale se as duas metades não se misturarem de novo. O RustCoreService
  // existe para comandos de domínio; se ele começar a acionar janela, updater ou rede, a
  // fronteira desaparece por dentro, sem nenhum arquivo novo aparecer.
  const core = readFileSync(new URL('../src/app/core/services/rust-core.service.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(core, /@tauri-apps\/plugin-|@tauri-apps\/api\/(window|path|event|shell|fs)/u,
    'capacidade de plataforma pertence a core/native, não à porta do núcleo de domínio');
});

test('a lista de atributos padrão é a mesma nos dois lados', () => {
  // A lista vive em dois lugares por necessidade: o Rust a usa para montar a
  // ficha ao criar a entidade, e a tela a usa para desenhar o formulário. A
  // alternativa — o frontend mandar a lista no comando — deixaria o cliente
  // decidir o formato do dado gravado. Este teste é o que impede a duplicação
  // de virar divergência silenciosa.
  const ts = readFileSync(new URL('../src/app/core/models/index.ts', import.meta.url), 'utf8');
  const rs = readFileSync(new URL('../src-tauri/src/domain/entity.rs', import.meta.url), 'utf8');

  const tsBlock = ts.slice(ts.indexOf('export const DEFAULT_ATTRIBUTES'));
  const tsLists = new Map();
  for (const match of tsBlock.slice(0, tsBlock.indexOf('\n};')).matchAll(/'([^']+)':\s*\[([^\]]*)\]/gu)) {
    tsLists.set(match[1], [...match[2].matchAll(/'([^']+)'/gu)].map((item) => item[1]));
  }

  const rsBlock = rs.slice(rs.indexOf('pub const DEFAULT_ATTRIBUTES'));
  const rsLists = new Map();
  for (const match of rsBlock.slice(0, rsBlock.indexOf('\n];')).matchAll(/\(\s*"([^"]+)",\s*&\[([^\]]*)\]/gu)) {
    rsLists.set(match[1], [...match[2].matchAll(/"([^"]+)"/gu)].map((item) => item[1]));
  }

  assert.ok(tsLists.size > 0 && rsLists.size > 0, 'nenhuma das duas listas foi encontrada');
  assert.deepEqual([...rsLists.keys()].sort(), [...tsLists.keys()].sort(), 'os tipos precisam ser os mesmos');
  for (const [kind, keys] of tsLists) {
    assert.deepEqual(rsLists.get(kind), keys, `os atributos de ${kind} divergiram entre Rust e TypeScript`);
  }
});

test('o patch que o gateway envia casa campo a campo com o struct do Rust', () => {
  // Bug real encontrado em revisão: `UpdateEntityInput` era um
  // `Partial<Pick<Entity, ...>>`, então mandava `canon_status`. O struct
  // `EntityUpdate` tem `rename_all = "camelCase"` e espera `canonStatus` —
  // serde ignora a chave desconhecida **em silêncio**, o comando devolve
  // sucesso e o dado não é gravado. A tela mostrava o estado novo, o banco
  // guardava o antigo, e nada acusava até o usuário reabrir a ficha.
  //
  // Nomes de uma palavra só não têm como divergir; o risco mora nos compostos.
  const pairs = [
    { rust: 'EntityUpdate', file: 'domain/entity.rs', ts: 'UpdateEntityInput', tsFile: 'features/entities/gateways/entity.gateway.ts' },
    { rust: 'UniverseUpdate', file: 'domain/universe.rs', ts: 'UpdateUniverseInput', tsFile: 'features/library/gateways/universe.gateway.ts' },
    { rust: 'StoryUpdate', file: 'domain/manuscript.rs', ts: 'UpdateStoryInput', tsFile: 'features/manuscript/gateways/manuscript.gateway.ts' },
    { rust: 'BookUpdate', file: 'domain/manuscript.rs', ts: 'UpdateBookInput', tsFile: 'features/manuscript/gateways/manuscript.gateway.ts' },
  ];

  const camel = (name) => name.replace(/_(\w)/gu, (_, letter) => letter.toUpperCase());

  for (const pair of pairs) {
    const rs = readFileSync(new URL(`../src-tauri/src/${pair.file}`, import.meta.url), 'utf8');
    const structStart = rs.indexOf(`pub struct ${pair.rust} {`);
    assert.ok(structStart > 0, `${pair.rust} não encontrado em ${pair.file}`);
    const renamed = rs.slice(Math.max(0, structStart - 200), structStart).includes('rename_all = "camelCase"');
    const structBody = rs.slice(structStart, rs.indexOf('\n}', structStart));
    const rustFields = [...structBody.matchAll(/pub (\w+):/gu)]
      .map((match) => (renamed ? camel(match[1]) : match[1]))
      .sort();

    const ts = readFileSync(new URL(`../src/app/${pair.tsFile}`, import.meta.url), 'utf8');
    const interfaceStart = ts.indexOf(`interface ${pair.ts} {`);
    assert.ok(interfaceStart > 0, `${pair.ts} precisa ser uma interface — um Pick<> traz nome de coluna junto`);
    const interfaceBody = ts.slice(interfaceStart, ts.indexOf('\n}', interfaceStart));
    const tsFields = [...interfaceBody.matchAll(/^\s{2}(\w+)\??:/gmu)].map((match) => match[1]).sort();

    assert.deepEqual(
      tsFields,
      rustFields,
      `${pair.ts} e ${pair.rust} divergiram: o serde descartaria a chave desconhecida sem erro`,
    );
  }
});

test('o formato canonico de imagem e o mesmo no Rust e na extensao do Tiptap', () => {
  // ADR 0010, decisão A da NH-065. O documento persistido guarda referência:
  //
  //   <img data-narrahub-blob="<64 hex>" data-mime-type="image/png" alt="rosto.png">
  //
  // Os dois lados escrevem esses nomes por conta própria: o Rust nas constantes de
  // `blob_document.rs`, a extensão no `parseHTML`/`renderHTML` de cada atributo. Divergir é
  // silencioso e caro — o transformador gravaria `data-narrahub-blob` e o editor leria outra
  // coisa, então toda imagem migrada desapareceria do capítulo ao carregar. Sem erro nenhum:
  // o Tiptap simplesmente não reconheceria o atributo.
  const rust = readFileSync(new URL('../src-tauri/src/infrastructure/blob_document.rs', import.meta.url), 'utf8');
  const editor = readFileSync(new URL('../src/app/features/writing/writing-editor.component.ts', import.meta.url), 'utf8');

  const constante = (nome) => {
    const achado = rust.match(new RegExp(`pub const ${nome}: &str = "([^"]+)"`, 'u'));
    assert.ok(achado, `não achei ${nome} em blob_document.rs; a varredura quebrou`);
    return achado[1];
  };
  const atributoDoBlob = constante('ATTR_BLOB');
  const atributoDoMime = constante('ATTR_MIME');

  // A varredura acha mesmo o que deveria? Sem isto o gate passaria por vácuo.
  assert.equal(atributoDoBlob, 'data-narrahub-blob');
  assert.equal(atributoDoMime, 'data-mime-type');

  for (const atributo of [atributoDoBlob, atributoDoMime]) {
    assert.ok(
      editor.includes(`getAttribute('${atributo}')`),
      `a extensão do Tiptap não lê ${atributo}. O transformador grava esse atributo, e o `
        + `editor descartaria a imagem ao carregar o capítulo.`,
    );
    assert.ok(
      editor.includes(`'${atributo}':`),
      `a extensão do Tiptap não escreve ${atributo} de volta`,
    );
  }
});

test('o seletor da imagem no Tiptap nao exige src', () => {
  // Um documento já convertido não tem `src` — ele tem só a referência. Enquanto o seletor
  // fosse `img[src]`, o Tiptap não reconheceria o elemento e o descartaria inteiro ao
  // carregar: a imagem do escritor sumiria da tela, e o próximo salvamento gravaria o
  // capítulo já sem ela.
  const editor = readFileSync(new URL('../src/app/features/writing/writing-editor.component.ts', import.meta.url), 'utf8');
  const extensao = editor.slice(editor.indexOf('const InlineImage'), editor.indexOf('function createCharacterAvatarExtension'));
  assert.ok(extensao.length > 200, 'não achei a extensão InlineImage; a varredura quebrou');

  assert.ok(
    !/tag:\s*'img\[src\]'/u.test(extensao),
    "o seletor voltou a ser 'img[src]', e isso descarta todo documento já convertido",
  );
  assert.ok(
    /tag:\s*'img'/u.test(extensao),
    "o seletor precisa ser 'img' para reconhecer os dois formatos",
  );
});

test('a extensao do Tiptap nao serializa src de volta', () => {
  // O `src` existe em memória, preenchido pelo resolvedor em runtime. Serializá-lo gravaria
  // uma URL local — específica daquela máquina — dentro do documento, e o acervo restaurado
  // noutro aparelho apontaria para um caminho que não existe. É o item 4 do contrato do ADR
  // 0010: nenhum caminho absoluto é persistido.
  const editor = readFileSync(new URL('../src/app/features/writing/writing-editor.component.ts', import.meta.url), 'utf8');
  const extensao = editor.slice(editor.indexOf('const InlineImage'), editor.indexOf('function createCharacterAvatarExtension'));

  const bloco = extensao.slice(extensao.indexOf('src: {'));
  const fim = bloco.indexOf('},');
  assert.ok(fim > 0, 'não achei o atributo src na extensão; a varredura quebrou');
  const declaracao = bloco.slice(0, fim);

  assert.ok(
    /renderHTML:\s*\(\)\s*=>\s*\(\{\}\)/u.test(declaracao),
    'o `src` voltou a ser serializado. A URL resolvida em runtime iria para o banco:\n  '
      + declaracao.trim(),
  );
});

test('a imagem inserida pelo editor nasce como referencia de blob', () => {
  // ADR 0010. O caminho antigo era `File → readAsDataURL → <img src="data:...">`, e ele punha
  // os bytes dentro do texto do capitulo: o documento crescia dezenas de vezes, o gatilho de
  // revisao copiava tudo a cada salvamento, e o evento assinado levava a base64 para todos os
  // aparelhos.
  //
  // O gate e textual porque a propriedade e sobre o CAMINHO, e o caminho e uma chamada. Um
  // teste de comportamento aqui precisaria de Tauri, FileReader e disco.
  const editor = readFileSync(new URL('../src/app/features/writing/writing-editor.component.ts', import.meta.url), 'utf8');
  const inicio = editor.indexOf('async importImage(');
  assert.ok(inicio > 0, 'nao achei importImage; a varredura quebrou');
  // Sem os comentários: a explicação do que mudou CITA o caminho antigo, e um gate que lê
  // comentário reprovaria a própria documentação da correção.
  const corpo = editor
    .slice(inicio, editor.indexOf('\n  }', inicio))
    .split('\n')
    .filter((linha) => !linha.trim().startsWith('//'))
    .join('\n');

  assert.ok(
    /blobs\.publish\(/u.test(corpo),
    'a insercao de imagem tem que publicar no blob store antes de tocar no documento',
  );
  assert.ok(
    /blobHash/u.test(corpo),
    'o node inserido tem que carregar a referencia por hash',
  );
  assert.ok(
    !/readAsDataURL|fileToDataUrl|data:image/u.test(corpo),
    `a insercao voltou a produzir data URL:\n${corpo}`,
  );
});

test('o editor nao tem mais caminho de data URL para persistencia', () => {
  // O helper `fileToDataUrl` existia so para o caminho antigo. Deixa-lo no arquivo seria
  // deixar a porta destrancada: a proxima pessoa que precisar inserir imagem acha a funcao
  // pronta e usa.
  const editor = readFileSync(new URL('../src/app/features/writing/writing-editor.component.ts', import.meta.url), 'utf8');
  const linhas = editor
    .split('\n')
    .map((linha, indice) => ({ linha, numero: indice + 1 }))
    .filter(({ linha }) => /readAsDataURL|fileToDataUrl/u.test(linha))
    .filter(({ linha }) => !linha.trim().startsWith('//'));

  assert.deepEqual(
    linhas,
    [],
    'o editor voltou a ter caminho de data URL fora de comentario:\n'
      + linhas.map(({ numero, linha }) => `  ${numero}: ${linha.trim()}`).join('\n'),
  );
});

test('o servico de blob nao devolve caminho de arquivo ao frontend', () => {
  // Invariante do ADR 0010: nenhum caminho absoluto atravessa a fronteira. O mesmo capitulo
  // tem que funcionar no Windows e no Android, e caminho e exatamente o que nao viaja --
  // restaurar um backup noutra maquina deixaria toda imagem apontando para o nada.
  const servico = readFileSync(new URL('../src/app/core/native/blob.service.ts', import.meta.url), 'utf8');
  assert.ok(/blob_put/u.test(servico) && /blob_read/u.test(servico), 'a varredura quebrou');

  for (const proibido of ['path_for', 'app_data', 'blobPath', 'filePath']) {
    assert.ok(
      !servico.includes(proibido),
      `o servico de blob menciona ${proibido}: caminho nao atravessa a fronteira`,
    );
  }
  assert.ok(
    /URL\.createObjectURL/u.test(servico),
    'a URL de exibicao e montada em memoria, na propria aba',
  );
});

test('o arranque prepara o acervo entre as migrations e o primeiro consumo', () => {
  // ADR 0010. O backfill existia desde a fatia 5 e nao tinha chamador -- a mesma lacuna que a
  // revisao da etapa 2.5 apontou para `load_or_create`: funciona em teste e nunca roda no
  // aplicativo.
  //
  // A ordem importa e por isso o gate mede posicao, nao so presenca:
  //
  //   db.init()          o plugin-sql aplica as migrations
  //   prepareArchive()   converte o legado de midia E adota o acervo (etapa C)
  //   universes.load()   primeiro consumo do acervo
  //
  // A ordem DENTRO da preparacao (midia antes da adocao) e do Rust, e tem gate la
  // (`application::arranque`). Aqui o que se mede e a posicao da chamada no arranque.
  //
  // Chamar depois do primeiro consumo deixaria a tela ler um acervo que ainda tem base64
  // dentro, e `update_chapter` recusaria o proximo salvamento.
  const arranque = readFileSync(new URL('../src/app/bootstrap/app-bootstrap.service.ts', import.meta.url), 'utf8');

  // O pool abre (e o plugin migra) dentro de openDatabaseSafely, com backup antes — ver
  // tests/migration-safety.test.mjs. A posição que importa é a da chamada no arranque.
  const migrations = arranque.indexOf('this.openDatabaseSafely()');
  const assets = arranque.indexOf('this.blobs.prepareArchive()');
  const consumo = arranque.indexOf('this.universes.load()');

  assert.ok(migrations > 0, 'nao achei a abertura do pool; a varredura quebrou');
  assert.ok(
    assets > 0,
    'o arranque nao chama `prepareArchive`. Sem isso o backfill e a adocao voltam a ser '
      + 'codigo sem chamador: um acervo antigo abriria com base64 dentro do banco e sem '
      + 'passado causal nenhum.',
  );
  assert.ok(consumo > 0, 'nao achei o primeiro consumo do acervo');

  assert.ok(
    migrations < assets && assets < consumo,
    'a fronteira de assets tem que ficar DEPOIS das migrations e ANTES do primeiro consumo:'
      + `\n  db.init()        em ${migrations}`
      + `\n  prepareArchive() em ${assets}`
      + `\n  universes.load() em ${consumo}`,
  );
});

test('uma falha no preparo do acervo interrompe o arranque em vez de ser engolida', () => {
  // Ate a etapa C, a chamada era so de midia e ficava dentro de um try/catch que registrava e
  // seguia: pendencia de midia e problema de midia, e travar a abertura por uma imagem antiga
  // ilegivel transformaria isso em perda de acesso ao texto.
  //
  // Com a adocao na mesma chamada, a regra muda. Pendencia de midia continua nao travando nada
  // (o Rust devolve Ok, com `sincronizacaoDisponivel: false` e o motivo); mas um ERRO significa
  // acervo sem passado causal, e seguir abriria o aplicativo sobre um estado que a
  // sincronizacao nao sabe descrever. O banco fica preservado, em recuperacao.
  const arranque = readFileSync(new URL('../src/app/bootstrap/app-bootstrap.service.ts', import.meta.url), 'utf8');
  const inicio = arranque.indexOf('this.blobs.prepareArchive()');
  assert.ok(inicio > 0, 'nao achei a chamada de preparo do acervo');

  // Nada de try/catch local em volta da chamada: o erro tem que subir para o catch do arranque,
  // que mostra a causa na tela.
  const trecho = arranque.slice(Math.max(0, inicio - 200), inicio + 200);
  assert.ok(
    !/try\s*\{[^]*this\.blobs\.prepareArchive\(\)/u.test(trecho),
    'a chamada voltou a ser engolida por um try/catch local: uma adocao que falha precisa '
      + 'interromper o arranque, nao virar linha de log.',
  );

  // E o resultado sem sincronizacao precisa ser dito, nao ignorado.
  assert.ok(
    arranque.includes('sincronizacaoDisponivel'),
    'o arranque ignora o veredito de sincronizacao do preparo do acervo.',
  );
});

test('o panorama do Sync V2 tem os mesmos campos no Rust e no TypeScript', () => {
  // Etapa 14, fatia 2. O `Panorama` viaja por IPC com `serde(rename_all = "camelCase")`, e o
  // frontend declara a mesma forma a mao. Sao dois arquivos que precisam concordar e que
  // nenhum compilador compara: renomear um campo no Rust deixaria o TypeScript compilando e
  // lendo `undefined` -- que na tela aparece como zero, ou seja, como "sincronizado".
  const rust = readFileSync(
    new URL('../src-tauri/src/application/sync_panorama.rs', import.meta.url), 'utf8');
  const ts = readFileSync(
    new URL('../src/app/core/native/sync-v2.service.ts', import.meta.url), 'utf8');

  const structRust = (nome) => {
    const inicio = rust.indexOf(`pub struct ${nome} {`);
    assert.ok(inicio > 0, `nao achei o struct ${nome}; a varredura quebrou`);
    const corpo = rust.slice(inicio, rust.indexOf('\n}', inicio));
    return [...corpo.matchAll(/^\s{4}pub ([a-z0-9_]+):/gmu)].map((m) => m[1]);
  };

  const interfaceTs = (nome) => {
    const inicio = ts.indexOf(`export interface ${nome} {`);
    assert.ok(inicio > 0, `nao achei a interface ${nome}; a varredura quebrou`);
    const corpo = ts.slice(inicio, ts.indexOf('\n}', inicio));
    return [...corpo.matchAll(/^\s{2}([A-Za-z0-9_]+)\??:/gmu)].map((m) => m[1]);
  };

  const camel = (snake) => snake.replace(/_([a-z0-9])/gu, (_, c) => c.toUpperCase());

  for (const [nomeRust, nomeTs] of [
    ['Panorama', 'SyncV2Panorama'],
    ['AparelhoConhecido', 'KnownDevice'],
    ['PendenciaDeOrigem', 'OriginPending'],
  ]) {
    const esperados = structRust(nomeRust).map(camel).sort();
    const declarados = interfaceTs(nomeTs).sort();
    assert.ok(esperados.length >= 4, `${nomeRust}: a varredura achou campos de menos`);
    assert.deepStrictEqual(
      declarados, esperados,
      `${nomeRust} (Rust) e ${nomeTs} (TypeScript) divergiram. O IPC serializa em camelCase, `
        + 'e campo que o frontend le errado chega como `undefined` -- que na tela vira zero.',
    );
  }
});

test('o servico do Sync V2 nao fala com o Sync V1', () => {
  // Decisao registrada: congelar e substituir, sem coexistir. Um acervo com parte das
  // escritas vindas do snapshot do V1 e parte da causalidade do V2 teria estado cuja origem
  // o V2 nao consegue explicar.
  const bruto = readFileSync(
    new URL('../src/app/core/native/sync-v2.service.ts', import.meta.url), 'utf8');

  // Sem os comentários. A primeira versão deste gate reprovou o próprio arquivo porque a
  // documentação dele NOMEIA os comandos do V1 para dizer que não os usa -- e foi o mesmo
  // erro que a etapa 13 cometeu num gate de documentação: varrer prosa como se fosse código.
  const ts = bruto
    .replace(/\/\*[\s\S]*?\*\//gu, '')
    .replace(/^\s*\/\/.*$/gmu, '');

  for (const proibido of ['sync_start', 'sync_stop', 'sync_connect', 'sync_status']) {
    assert.ok(
      !ts.includes(proibido),
      `\`${proibido}\` e comando do Sync V1. O servico do V2 nao se apoia nele.`,
    );
  }

  assert.ok(
    ts.includes("invoke<SyncV2Panorama>('sync_v2_panorama')"),
    'o servico precisa chamar o comando do V2; sem isso ele e um tipo sem porta.',
  );
});

test('o comando do Sync V2 esta registrado no invoke_handler', () => {
  // A lacuna que a etapa 14 encontrou foi exatamente esta: 108 comandos registrados e nenhum
  // do V2. Um comando que existe e nao esta no `invoke_handler` e codigo sem porta -- o mesmo
  // defeito que a revisao da etapa 2.5 apontou em `load_or_create` e a da 13 no backfill.
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  const inicio = lib.indexOf('invoke_handler');
  assert.ok(inicio > 0, 'nao achei o invoke_handler');
  const lista = lib.slice(inicio, lib.indexOf('])', inicio));

  assert.ok(
    lista.includes('sync_v2_commands::sync_v2_panorama'),
    'o comando do panorama do V2 nao esta registrado. Comando fora do `invoke_handler` nao '
      + 'existe para o frontend.',
  );
});

test('os tipos da sessao do Sync V2 tem os mesmos campos no Rust e no TypeScript', () => {
  // Etapa 14, fatia 4. Mesmo motivo do gate do panorama: o IPC serializa em camelCase, o
  // frontend declara a forma a mao, e nenhum compilador compara os dois. Um campo renomeado
  // no Rust chega como `undefined` -- e numa tela de sincronizacao `undefined` vira "0
  // imagens recebidas", que parece sucesso.
  const ler = (relativo) => readFileSync(new URL(relativo, import.meta.url), 'utf8');
  const ts = ler('../src/app/core/native/sync-v2.service.ts');

  const camposRust = (fonte, nome) => {
    const inicio = fonte.indexOf(`pub struct ${nome} {`);
    assert.ok(inicio >= 0, `nao achei o struct ${nome}; a varredura quebrou`);
    const corpo = fonte.slice(inicio, fonte.indexOf('\n}', inicio));
    return [...corpo.matchAll(/^\s{4}pub ([a-z0-9_]+):/gmu)].map((m) => m[1]);
  };
  const camposTs = (nome) => {
    const inicio = ts.indexOf(`export interface ${nome} {`);
    assert.ok(inicio >= 0, `nao achei a interface ${nome}; a varredura quebrou`);
    const corpo = ts.slice(inicio, ts.indexOf('\n}', inicio));
    return [...corpo.matchAll(/^\s{2}([A-Za-z0-9_]+)\??:/gmu)].map((m) => m[1]);
  };
  const camel = (snake) => snake.replace(/_([a-z0-9])/gu, (_, c) => c.toUpperCase());

  for (const [arquivo, nomeRust, nomeTs] of [
    ['../src-tauri/src/interface/tauri/sync_v2_commands.rs', 'EstadoDaEscutaV2', 'SyncV2ListenState'],
    ['../src-tauri/src/application/sync_sessao.rs', 'ResultadoDaSessao', 'SyncSessionResult'],
    ['../src-tauri/src/application/sync_pin_pairing.rs', 'Parceiro', 'SyncPartner'],
  ]) {
    const esperados = camposRust(ler(arquivo), nomeRust).map(camel).sort();
    const declarados = camposTs(nomeTs).sort();
    assert.ok(esperados.length >= 2, `${nomeRust}: a varredura achou campos de menos`);
    assert.deepStrictEqual(
      declarados, esperados,
      `${nomeRust} (Rust) e ${nomeTs} (TypeScript) divergiram.`,
    );
  }

  // O papel e um enum serializado em camelCase; o tipo do TS precisa ter os mesmos valores.
  const sessao = ler('../src-tauri/src/application/sync_sessao.rs');
  const corpoPapel = sessao.slice(sessao.indexOf('pub enum Papel {'), sessao.indexOf('\n}', sessao.indexOf('pub enum Papel {')));
  const variantes = [...corpoPapel.matchAll(/^\s{4}([A-Z][A-Za-z]+),/gmu)]
    .map((m) => m[1].charAt(0).toLowerCase() + m[1].slice(1)).sort();
  const declaradas = [...ts.match(/export type SyncRole = ([^;]+);/u)[1].matchAll(/'([a-z]+)'/gu)]
    .map((m) => m[1]).sort();
  assert.deepStrictEqual(declaradas, variantes, 'o papel da sessao divergiu entre Rust e TypeScript');
});

test('os sete comandos do Sync V2 estao registrados e a porta chama cada um', () => {
  // A lacuna da etapa 14 foi exatamente comando sem porta. Cada comando precisa estar no
  // `invoke_handler` E ser chamado pela porta do frontend -- um sem o outro e codigo sem
  // chamador, de um lado ou do outro da fronteira.
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  const inicio = lib.indexOf('invoke_handler');
  const lista = lib.slice(inicio, lib.indexOf('])', inicio));
  const ts = readFileSync(new URL('../src/app/core/native/sync-v2.service.ts', import.meta.url), 'utf8');

  for (const comando of [
    'sync_v2_panorama',
    'sync_v2_estado',
    'sync_v2_escuta_iniciar',
    'sync_v2_escuta_parar',
    'sync_v2_pin_novo',
    'sync_v2_parear',
    'sync_v2_sincronizar',
  ]) {
    assert.ok(
      lista.includes(`sync_v2_commands::${comando}`),
      `\`${comando}\` nao esta no invoke_handler. Comando fora dele nao existe para o frontend.`,
    );
    assert.ok(
      ts.includes(`'${comando}'`),
      `a porta do frontend nao chama \`${comando}\`. Comando registrado sem chamador e a lacuna que a etapa 14 encontrou.`,
    );
  }

  assert.ok(
    lib.includes('sync_v2_commands::EstadoV2::default()'),
    'o estado do Sync V2 nao e gerenciado pelo Tauri; os comandos de escuta falhariam ao abrir.',
  );
});

test('a tela nao deixa o Sync V1 e o Sync V2 ativos ao mesmo tempo', () => {
  // Decisao registrada: congelar o V1 e substituir, sem coexistir. A trava fica na tela
  // porque o Rust do V2 nao pode depender do V1. Este gate cobra os dois lados da trava.
  const html = readFileSync(
    new URL('../src/app/features/settings/settings-page.component.html', import.meta.url), 'utf8');
  const store = readFileSync(
    new URL('../src/app/features/settings/state/settings.store.ts', import.meta.url), 'utf8');

  assert.ok(
    /\(click\)="startSync\(\)"/u.test(html) && /store\.syncV1Blocked\(\)[^"]*"\s*\(click\)="startSync\(\)"/u.test(html),
    'o botao de ligar o V1 precisa ficar travado enquanto a escuta do V2 estiver ativa.',
  );
  assert.ok(
    /store\.syncV2Blocked\(\)[^"]*"\s*\(click\)="startSyncV2\(\)"/u.test(html),
    'o botao de ligar a escuta do V2 precisa ficar travado enquanto o V1 estiver ativo.',
  );
  assert.ok(
    /syncV1Blocked\(\): boolean \{\s*return this\.syncV2State\(\)\.escutando;/u.test(store)
      && /syncV2Blocked\(\): boolean \{\s*return this\.syncStatus\(\)\.running;/u.test(store),
    'as travas precisam olhar o estado real de cada mecanismo.',
  );
});

test('os tipos da atualizacao do Android tem os mesmos campos no Rust e no TypeScript', () => {
  // Mesmo motivo dos outros espelhos: o IPC serializa em camelCase e nenhum compilador compara.
  // Aqui um campo divergente faria a tela dizer "atualizado" com versao nova publicada.
  const ler = (relativo) => readFileSync(new URL(relativo, import.meta.url), 'utf8');
  const ts = ler('../src/app/core/native/android-update.service.ts');
  const camposRust = (fonte, nome) => {
    const inicio = fonte.indexOf(`pub struct ${nome} {`);
    assert.ok(inicio >= 0, `nao achei o struct ${nome}`);
    const linhas = fonte.slice(inicio, fonte.indexOf('\n}', inicio)).split('\n');
    const campos = [];
    for (let i = 0; i < linhas.length; i += 1) {
      const campo = /^\s{4}pub ([a-z0-9_]+):/u.exec(linhas[i]);
      // Campo com #[serde(skip)] nao viaja para a tela.
      if (campo && !/#\[serde\(skip\)\]/u.test(linhas[i - 1] || '')) campos.push(campo[1]);
    }
    return campos;
  };
  const camposTs = (nome) => {
    const inicio = ts.indexOf(`export interface ${nome} {`);
    assert.ok(inicio >= 0, `nao achei a interface ${nome}`);
    const corpo = ts.slice(inicio, ts.indexOf('\n}', inicio));
    return [...corpo.matchAll(/^\s{2}([A-Za-z0-9_]+)\??:/gmu)].map((m) => m[1]);
  };
  const camel = (snake) => snake.replace(/_([a-z0-9])/gu, (_, c) => c.toUpperCase());
  for (const [arquivo, nomeRust, nomeTs] of [
    ['../src-tauri/src/application/atualizacao_android.rs', 'Novidade', 'AndroidUpdateNews'],
    ['../src-tauri/src/interface/tauri/android_update_commands.rs', 'VerificacaoAndroid', 'AndroidUpdateCheck'],
    ['../src-tauri/src/interface/tauri/android_update_commands.rs', 'ProgressoDoDownload', 'AndroidUpdateProgress'],
  ]) {
    assert.deepStrictEqual(
      camposTs(nomeTs).sort(),
      camposRust(ler(arquivo), nomeRust).map(camel).sort(),
      `${nomeRust} (Rust) e ${nomeTs} (TypeScript) divergiram`,
    );
  }
});

test('os comandos de atualizacao do Android nao recebem URL, caminho nem hash da tela', () => {
  // O Rust baixa e instala o que ele proprio verificou. Se a tela pudesse passar URL ou caminho,
  // um frontend comprometido faria o app baixar ou instalar um arquivo arbitrario.
  const fonte = readFileSync(new URL('../src-tauri/src/interface/tauri/android_update_commands.rs', import.meta.url), 'utf8');
  const assinaturas = fonte.split('#[tauri::command]').slice(1).map((resto) => resto.split('{')[0].split('->')[0]);
  assert.equal(assinaturas.length, 4, 'esperava os quatro comandos da atualizacao do Android');
  for (const assinatura of assinaturas) {
    for (const proibido of [/\burl\b/iu, /\bcaminho\b/iu, /\bpath\b/iu, /\bsha/iu, /\bhash\b/iu, /:\s*String\b/u]) {
      assert.ok(!proibido.test(assinatura), `um comando passou a receber ${proibido} da tela:\n${assinatura}`);
    }
  }
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  for (const comando of ['android_update_supported', 'android_update_check', 'android_update_download', 'android_update_install']) {
    assert.ok(lib.includes(`android_update_commands::${comando}`), `${comando} fora do invoke_handler`);
  }
});

test('os DTOs dos conflitos (etapa F) tem os mesmos campos no Rust e no TypeScript', () => {
  // Mesmo motivo dos gates do Sync V2: o IPC serializa em camelCase e ninguém compara as duas
  // declarações. Um campo renomeado chega como `undefined` -- e numa tela de conflito isso vira
  // uma versão vazia, que o escritor pode escolher achando que é a certa.
  const ler = (relativo) => readFileSync(new URL(relativo, import.meta.url), 'utf8');
  const ts = ler('../src/app/core/native/sync-conflicts.service.ts');
  const camposRust = (fonte, nome) => {
    const inicio = fonte.indexOf(`pub struct ${nome} {`);
    assert.ok(inicio >= 0, `nao achei o struct ${nome}; a varredura quebrou`);
    const corpo = fonte.slice(inicio, fonte.indexOf('\n}', inicio));
    return [...corpo.matchAll(/^\s{4}pub ([a-z0-9_]+):/gmu)].map((m) => m[1]);
  };
  const camposTs = (nome) => {
    const inicio = ts.indexOf(`export interface ${nome} {`);
    assert.ok(inicio >= 0, `nao achei a interface ${nome}; a varredura quebrou`);
    const corpo = ts.slice(inicio, ts.indexOf('\n}', inicio));
    return [...corpo.matchAll(/^\s{2}([A-Za-z0-9_]+)\??:/gmu)].map((m) => m[1]);
  };
  const camel = (snake) => snake.replace(/_([a-z0-9])/gu, (_, c) => c.toUpperCase());
  const conflitos = '../src-tauri/src/application/conflitos.rs';
  for (const [arquivo, nomeRust, nomeTs] of [
    [conflitos, 'FiltroDeConflitos', 'ConflictFilter'],
    [conflitos, 'ResumoDoConflito', 'ConflictSummary'],
    [conflitos, 'Campo', 'ConflictField'],
    [conflitos, 'LadoDoConflito', 'ConflictSide'],
    [conflitos, 'Diferenca', 'ConflictDifference'],
    [conflitos, 'LinhaDeDiff', 'ConflictDiffLine'],
    [conflitos, 'AcaoDisponivel', 'ConflictAction'],
    [conflitos, 'DetalheDoConflito', 'ConflictDetail'],
    [conflitos, 'AvisoDeEpoca', 'EpochNotice'],
    ['../src-tauri/src/application/resolucao_divergencia.rs', 'ResultadoDaResolucao', 'ConflictResolutionResult'],
  ]) {
    const esperados = camposRust(ler(arquivo), nomeRust).map(camel).sort();
    assert.ok(esperados.length >= 2, `${nomeRust}: a varredura achou campos de menos`);
    assert.deepStrictEqual(camposTs(nomeTs).sort(), esperados, `${nomeRust} (Rust) e ${nomeTs} (TypeScript) divergiram.`);
  }

  // A ação é um enum com `tag = "tipo"`: as variantes, em camelCase, são o tipo do TS.
  const resolucao = ler('../src-tauri/src/application/resolucao_divergencia.rs');
  const inicio = resolucao.indexOf('pub enum Acao {');
  const corpo = resolucao.slice(inicio, resolucao.indexOf('\n}', inicio));
  const variantes = [...corpo.matchAll(/^\s{4}([A-Z][A-Za-z]+)[ ,{]/gmu)]
    .map((m) => m[1].charAt(0).toLowerCase() + m[1].slice(1)).sort();
  const declaradas = [...ts.match(/export type ConflictActionType =([^;]+);/u)[1].matchAll(/'([A-Za-z]+)'/gu)]
    .map((m) => m[1]).sort();
  assert.ok(variantes.length >= 8, 'a varredura das ações quebrou');
  assert.deepStrictEqual(declaradas, variantes, 'as ações de resolução divergiram entre Rust e TypeScript');
  assert.match(corpo, /#\[serde\(rename_all = "camelCase"\)\]\s*Renomear \{ tag_id/u, 'renomear precisa chegar como tagId');
});

test('os quatro comandos dos conflitos estao registrados e a porta chama cada um', () => {
  const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8');
  const inicio = lib.indexOf('invoke_handler');
  const lista = lib.slice(inicio, lib.indexOf('])', inicio));
  const ts = readFileSync(new URL('../src/app/core/native/sync-conflicts.service.ts', import.meta.url), 'utf8');
  for (const comando of [
    'sync_conflitos_listar',
    'sync_conflito_inspecionar',
    'sync_conflito_resolver',
    'sync_v2_aviso_de_epoca',
  ]) {
    assert.ok(lista.includes(`conflitos_commands::${comando}`), `\`${comando}\` nao esta no invoke_handler.`);
    assert.ok(ts.includes(`'${comando}'`), `a porta do frontend nao chama \`${comando}\`.`);
  }
});
