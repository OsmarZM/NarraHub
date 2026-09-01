import { readFile } from 'node:fs/promises';

// Este validador existia só para a pipeline de release, e conferia apenas os três
// manifests. Foi o suficiente para eles nunca divergirem entre si — e insuficiente para
// impedir o que de fato aconteceu: o README anunciou 0.7.4 enquanto o produto publicado
// era 0.9.1, por três releases seguidas, porque ninguém validava a documentação.
//
// A regra agora é que a versão é um fato único do repositório. Se um arquivo declara a
// versão do produto, ele entra aqui.

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const tauriConfig = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const cargoManifest = await readFile('src-tauri/Cargo.toml', 'utf8');
const cargoVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/mu)?.[1];

const versions = new Set([packageJson.version, tauriConfig.version, cargoVersion]);
if (versions.size !== 1 || versions.has(undefined)) {
  throw new Error(
    `Versões divergentes nos manifests: package=${packageJson.version}, tauri=${tauriConfig.version}, cargo=${cargoVersion}`,
  );
}

const version = packageJson.version;
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u.test(version)) {
  throw new Error(`A versão "${version}" não é SemVer válida.`);
}

// O README descreve o estado corrente do produto, não o histórico. O cabeçalho
// "## Versão X.Y.Z" é o que o leitor usa para saber o que está lendo.
const readme = await readFile('README.md', 'utf8');
const readmeVersion = readme.match(/^##\s+Versão\s+(\S+)\s*$/mu)?.[1];
if (!readmeVersion) {
  throw new Error('O README.md precisa de um cabeçalho "## Versão X.Y.Z" declarando a versão corrente.');
}
if (readmeVersion !== version) {
  throw new Error(`README.md anuncia ${readmeVersion}, mas os manifests estão em ${version}.`);
}

// O CHANGELOG guarda o histórico, então só a entrada mais recente precisa bater: uma
// release sem nota de mudança é uma release que o usuário não consegue interpretar.
const changelog = await readFile('CHANGELOG.md', 'utf8');
const changelogVersion = changelog.match(/^##\s+(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\b/mu)?.[1];
if (!changelogVersion) {
  throw new Error('O CHANGELOG.md precisa de uma entrada "## X.Y.Z" para a versão corrente.');
}
if (changelogVersion !== version) {
  throw new Error(
    `A entrada mais recente do CHANGELOG.md é ${changelogVersion}, mas os manifests estão em ${version}.`,
  );
}

// O PROJECT_STATE é a memória compartilhada de Claude, Codex e Gemini: os três leem esse
// arquivo antes de agir. Memória compartilhada desatualizada é pior que memória nenhuma,
// porque os três passam a raciocinar em cima do mesmo erro, com confiança. Em 2026-09-01 ele
// ainda anunciava 0.9.1 e citava uma branch já apagada.
const projectState = await readFile('docs/ai/PROJECT_STATE.md', 'utf8');
const stateVersion = projectState.match(/^\|\s*Versão corrente\s*\|\s*\*{0,2}(\S+?)\*{0,2}\s*\|/mu)?.[1];
if (!stateVersion) {
  throw new Error(
    'docs/ai/PROJECT_STATE.md precisa de uma linha "| Versão corrente | X.Y.Z |" na tabela de versão.',
  );
}
if (stateVersion !== version) {
  throw new Error(
    `docs/ai/PROJECT_STATE.md diz que a versão corrente é ${stateVersion}, mas os manifests estão em ${version}. ` +
      'Os agentes leem esse arquivo como fonte da verdade — corrija antes de seguir.',
  );
}

console.log(`Versão ${version} consistente em package.json, Cargo.toml, tauri.conf.json, README.md, CHANGELOG.md e PROJECT_STATE.md.`);
