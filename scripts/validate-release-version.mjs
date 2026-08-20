import { readFile } from 'node:fs/promises';

const packageJson = JSON.parse(await readFile('package.json', 'utf8'));
const tauriConfig = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const cargoManifest = await readFile('src-tauri/Cargo.toml', 'utf8');
const cargoVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/mu)?.[1];
const versions = new Set([packageJson.version, tauriConfig.version, cargoVersion]);
if (versions.size !== 1 || versions.has(undefined)) {
  throw new Error(`Versões divergentes: package=${packageJson.version}, tauri=${tauriConfig.version}, cargo=${cargoVersion}`);
}
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/u.test(packageJson.version)) throw new Error('A versão não é SemVer válida.');
console.log(`Versão ${packageJson.version} consistente nos três manifests.`);
