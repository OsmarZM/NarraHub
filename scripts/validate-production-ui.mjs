import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';

const outputDirectory = path.resolve('dist/narrahub-app/browser');
const indexPath = path.join(outputDirectory, 'index.html');
const index = await readFile(indexPath, 'utf8');

if (/rel=["']stylesheet["'][^>]*media=["']print["']/i.test(index)) {
  throw new Error('O CSS de producao ainda depende de media="print" e de um onload bloqueado pelo CSP do Tauri.');
}
if (/\sonload=["'][^"']*media/i.test(index)) {
  throw new Error('O CSS de producao nao pode depender de um manipulador onload inline.');
}

const stylesheetMatch = index.match(/<link[^>]+rel=["']stylesheet["'][^>]+href=["']([^"']+\.css)["'][^>]*>/i);
if (!stylesheetMatch) {
  throw new Error('O bundle de producao nao contem um stylesheet carregado diretamente.');
}

const stylesheetName = path.basename(stylesheetMatch[1]);
const outputFiles = await readdir(outputDirectory);
if (!outputFiles.includes(stylesheetName)) {
  throw new Error(`O stylesheet referenciado nao existe no bundle: ${stylesheetName}`);
}

const stylesheet = await readFile(path.join(outputDirectory, stylesheetName), 'utf8');
const requiredFragments = [
  ':root[data-theme=dark]',
  '.settings-shell',
  '.settings-nav',
  '.settings-shell .settings-grid>[hidden]',
];

for (const fragment of requiredFragments) {
  if (!stylesheet.includes(fragment)) {
    throw new Error(`O stylesheet de producao perdeu um seletor obrigatorio: ${fragment}`);
  }
}

console.log(`Interface de producao validada: ${stylesheetName} carrega diretamente e preserva tema e configuracoes.`);
