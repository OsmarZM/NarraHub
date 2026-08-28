import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const featuresDir = fileURLToPath(new URL('../src/app/features/', import.meta.url));
const interfaceDir = fileURLToPath(new URL('../src-tauri/src/interface/tauri/', import.meta.url));
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
  const commandSources = readdirSync(interfaceDir)
    .filter((name) => name.endsWith('.rs') && name !== 'mod.rs')
    .map((name) => ({ name, source: readFileSync(join(interfaceDir, name), 'utf8') }));

  const declared = new Map();
  for (const { name, source } of commandSources) {
    for (const match of source.matchAll(/#\[tauri::command\]\s*pub fn (\w+)/gu)) {
      declared.set(match[1], name.replace(/\.rs$/u, ''));
    }
  }
  assert.ok(declared.size > 0, 'nenhum comando encontrado em interface/tauri');

  const called = new Set();
  for (const path of rustGateways()) {
    const source = readFileSync(path, 'utf8');
    for (const match of source.matchAll(/core\.call<[^>]*>\('(\w+)'/gu)) called.add(match[1]);
  }
  assert.ok(called.size > 0, 'nenhuma chamada ao core encontrada — o teste ficaria vazio');

  for (const command of called) {
    const module = declared.get(command);
    assert.ok(module, `o frontend chama '${command}', que não existe em interface/tauri`);
    assert.ok(
      libSource.includes(`interface::tauri::${module}::${command},`),
      `'${command}' existe mas não está no invoke_handler de lib.rs — falharia só no clique do usuário`,
    );
  }
});

test('o core Rust não é chamado por fora do adaptador de gateway', () => {
  // `invoke()` espalhado por componente traria de volta o acoplamento que a
  // Fase 2 desmontou. A porta é RustCoreService, e quem a usa é adaptador.
  const offenders = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) { walk(path); continue; }
      if (!entry.name.endsWith('.ts')) continue;
      if (entry.name.startsWith('rust-') && entry.name.endsWith('.gateway.ts')) continue;
      const source = readFileSync(path, 'utf8');
      if (source.includes('RustCoreService')) offenders.push(path);
    }
  };
  walk(featuresDir);
  assert.deepEqual(offenders, [], `RustCoreService só pode ser usado por adaptador Rust: ${offenders.join(', ')}`);
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
