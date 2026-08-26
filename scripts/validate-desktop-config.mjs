import { readFile } from 'node:fs/promises';

const config = JSON.parse(await readFile('src-tauri/tauri.conf.json', 'utf8'));
const windowsConfig = JSON.parse(await readFile('src-tauri/tauri.windows.conf.json', 'utf8'));
const productionConfig = JSON.parse(await readFile('src-tauri/tauri.production.conf.json', 'utf8'));
const qualificationConfig = JSON.parse(await readFile('src-tauri/tauri.qualification.conf.json', 'utf8'));
const mainWindow = config.app?.windows?.[0];
const security = config.app?.security;

if (config.identifier !== 'com.narrahub.app.dev') {
  throw new Error('O perfil padrao precisa usar o identificador isolado de desenvolvimento.');
}
if (productionConfig.identifier !== 'com.narrahub.app' || productionConfig.productName !== 'NarraHub') {
  throw new Error('O perfil de producao precisa preservar a identidade instalada do NarraHub.');
}
if (config.identifier === productionConfig.identifier) {
  throw new Error('Desenvolvimento e producao nao podem compartilhar o mesmo diretorio de dados.');
}
const isolatedIdentifiers = new Set([config.identifier, productionConfig.identifier, qualificationConfig.identifier]);
if (isolatedIdentifiers.size !== 3 || qualificationConfig.identifier !== 'com.narrahub.app.qualification') {
  throw new Error('O perfil de qualificacao precisa permanecer isolado de desenvolvimento e producao.');
}

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
if (!windowsConfig.bundle?.externalBin?.includes('binaries/cloudflared')) {
  throw new Error('O instalador precisa incluir o sidecar cloudflared para compartilhamento temporario.');
}

console.log('Configuracao desktop validada: dados dev/producao/qualificacao isolados, janela segura, estilos Angular e sidecar configurados.');
