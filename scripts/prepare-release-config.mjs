import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

const publicKey = (process.env.TAURI_UPDATER_PUBLIC_KEY || '').trim();
if (!publicKey) throw new Error('TAURI_UPDATER_PUBLIC_KEY não foi configurada.');

const productionConfig = JSON.parse(await readFile('src-tauri/tauri.production.conf.json', 'utf8'));

const config = {
  ...productionConfig,
  bundle: { createUpdaterArtifacts: true },
  plugins: {
    updater: {
      pubkey: publicKey,
      endpoints: ['https://github.com/OsmarZM/NarraHub/releases/latest/download/latest.json'],
      windows: { installMode: 'passive' },
    },
  },
};

const target = path.resolve('src-tauri/tauri.release.conf.json');
await writeFile(target, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
console.log(`Configuração de release criada em ${target}.`);
