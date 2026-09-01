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
    // Ciclo de vida do pool SQLite: abre e fecha a conexão, não executa SQL.
    'core/services/database.service.ts',
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
