import { readFile } from 'node:fs/promises';

const config = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const mainWindow = config.app?.windows?.[0];
const security = config.app?.security;

if (!mainWindow) throw new Error('A janela principal nao esta configurada.');
if (mainWindow.maximized !== true) throw new Error('A janela principal deve iniciar maximizada.');
if (mainWindow.minWidth > 760 || mainWindow.minHeight > 520) {
  throw new Error('O tamanho minimo da janela ultrapassa o limite seguro para telas compactas.');
}

const disabledDirectives = security?.dangerousDisableAssetCspModification;
if (!Array.isArray(disabledDirectives) || disabledDirectives.length !== 1 || disabledDirectives[0] !== 'style-src') {
  throw new Error('A modificacao automatica do CSP deve ser desativada somente para style-src.');
}
if (!security.csp?.includes("style-src 'self' 'unsafe-inline'")) {
  throw new Error('O CSP precisa permitir os estilos dinamicos dos componentes Angular.');
}

console.log('Configuracao desktop validada: estilos Angular permitidos e janela dentro da area util.');
