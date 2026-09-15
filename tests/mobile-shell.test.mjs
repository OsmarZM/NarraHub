import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { BREAKPOINTS, MOBILE_MEDIA_QUERY, isMobileViewport } from '../src/app/shell/state/breakpoints.ts';

const ler = (relativo) => readFileSync(new URL(relativo, import.meta.url), 'utf8');

test('shell mobile: toque e lado menor até 760px, em pé ou deitado', () => {
  // Os quatro tamanhos de celular do plano, nos dois sentidos.
  for (const [width, height] of [[375, 667], [390, 844], [412, 915], [430, 932]]) {
    assert.ok(isMobileViewport({ width, height, coarsePointer: true }), `${width}x${height} em pé`);
    assert.ok(isMobileViewport({ width: height, height: width, coarsePointer: true }), `${height}x${width} deitado`);
  }
  // Desktop: janela estreita com mouse continua desktop; tela grande também.
  assert.equal(isMobileViewport({ width: 700, height: 900, coarsePointer: false }), false);
  assert.equal(isMobileViewport({ width: 1366, height: 768, coarsePointer: false }), false);
  // Tablet grande com toque: os dois lados acima de 760.
  assert.equal(isMobileViewport({ width: 800, height: 1280, coarsePointer: true }), false);
  assert.equal(isMobileViewport({ width: 1280, height: 800, coarsePointer: true }), false);
  // A media query diz a mesma coisa que a função.
  assert.equal(BREAKPOINTS.mobileShortSide, 760);
  assert.match(MOBILE_MEDIA_QUERY, /\(pointer: coarse\) and \(max-width: 760px\)/u);
  assert.match(MOBILE_MEDIA_QUERY, /\(pointer: coarse\) and \(max-height: 760px\)/u);
});

test('sem zoom no app: meta viewport, WebView e touch-action, sem depender de user-scalable', () => {
  const html = ler('../src/index.html');
  const meta = html.match(/<meta name="viewport" content="([^"]+)"/u)?.[1] ?? '';
  assert.match(meta, /viewport-fit=cover/u);
  assert.match(meta, /interactive-widget=resizes-content/u);
  assert.doesNotMatch(meta, /user-scalable\s*=\s*no/u, 'user-scalable=no não é a solução: o zoom fica fechado no WebView e no touch-action');

  const activity = ler('../src-tauri/gen/android/app/src/main/java/com/narrahub/app/MainActivity.kt');
  assert.match(activity, /setSupportZoom\(false\)/u);
  assert.match(activity, /builtInZoomControls = false/u);

  const mobile = ler('../src/styles/mobile.css');
  assert.match(mobile, /html\.nh-mobile\s*\{\s*touch-action:\s*pan-x pan-y;\s*\}/u, 'a página só rola: pinça e duplo toque não escalam');
  assert.match(ler('../src/styles.css'), /@import '\.\/styles\/mobile\.css';/u);
});

test('altura e safe area do mobile saem de tokens, não de 100vh nem env() espalhados', () => {
  const mobile = ler('../src/styles/mobile.css');
  assert.match(mobile, /--nh-app-height:\s*100dvh/u);
  for (const lado of ['top', 'right', 'bottom', 'left']) {
    assert.match(mobile, new RegExp(`--nh-safe-${lado}:\\s*env\\(safe-area-inset-${lado}, 0px\\)`, 'u'));
  }
  // O MainActivity é a fonte da verdade no APK: barras, recorte e teclado viram padding nativo.
  const activity = ler('../src-tauri/gen/android/app/src/main/java/com/narrahub/app/MainActivity.kt');
  assert.match(activity, /WindowInsetsCompat\.Type\.systemBars\(\) or WindowInsetsCompat\.Type\.displayCutout\(\)/u);
  assert.match(activity, /WindowInsetsCompat\.Type\.ime\(\)/u);
});

test('dois shells de apresentação, um só router-outlet', () => {
  // DesktopShell e MobileShell compartilham Router, stores e serviços; só a composição muda. Um
  // segundo outlet criaria duas instâncias da rota — dois editores do mesmo capítulo.
  const html = ler('../src/app/root-layout.component.html');
  assert.equal((html.match(/<router-outlet\b/gu) || []).length, 1);
  assert.match(html, /<ng-template #appContent>[\s\S]*<router-outlet \/>[\s\S]*<\/ng-template>/u);
  const mobile = html.indexOf('@if (viewport.isMobile())');
  const desktop = html.indexOf('} @else {', mobile);
  assert.ok(mobile >= 0 && desktop > mobile, 'o shell é escolhido pelo ViewportState');
  assert.match(html.slice(mobile, desktop), /<app-mobile-shell[\s\S]*<app-mobile-navigation[\s\S]*navigation/u);
  assert.match(html.slice(desktop), /<app-shell[\s\S]*<app-titlebar/u);
  assert.doesNotMatch(html.slice(desktop), /app-mobile-/u, 'o desktop não monta nada do mobile');
});

test('no celular a sidebar e o cabeçalho do universo não são renderizados', () => {
  const html = ler('../src/app/workspace-layout.component.html');
  assert.match(html, /@if \(!isFocusMode\(\) && !viewport\.isMobile\(\)\) \{\s*<app-universe-sidebar/u);
  assert.match(html, /@if \(!viewport\.isMobile\(\)\) \{\s*<header class="workspace-header">/u);
});

test('toda ação do cabeçalho do desktop existe no "•••" do celular, com o mesmo handler', () => {
  const html = ler('../src/app/workspace-layout.component.html');
  const ts = ler('../src/app/workspace-layout.component.ts');
  const inicio = html.indexOf('<div class="workspace-actions">');
  const cabecalho = html.slice(inicio, html.indexOf('</header>', inicio));
  const handlers = [...cabecalho.matchAll(/\(click\)="([a-zA-Z]+)\(/gu)].map((m) => m[1]);
  assert.ok(handlers.length >= 5, `esperava as ações do cabeçalho, achei ${handlers}`);
  const comeco = ts.indexOf('private readonly mobileActions');
  assert.ok(comeco >= 0, 'mobileActions sumiu do WorkspaceLayout');
  const mobile = ts.slice(comeco, ts.indexOf('return actions;', comeco));
  for (const handler of new Set(handlers)) {
    assert.ok(mobile.includes(`this.${handler}(`), `a ação "${handler}" do desktop não chegou ao celular`);
  }
});

test('todo destino da navegação tem cartão no navegador gestual', () => {
  // A sidebar some no celular; nada pode ficar inacessível.
  const tipos = ler('../src/app/core/navigation/app-navigation.ts');
  const ids = [...tipos.slice(tipos.indexOf('export type AppNavigationId'), tipos.indexOf(';', tipos.indexOf('export type AppNavigationId'))).matchAll(/'([a-z]+)'/gu)].map((m) => m[1]);
  assert.ok(ids.length >= 8);
  const root = ler('../src/app/root-layout.component.ts');
  const apresentacao = root.slice(root.indexOf('const MOBILE_PRESENTATION'));
  for (const id of ids) assert.ok(apresentacao.includes(`${id}: { label:`), `destino ${id} sem cartão`);
  assert.match(root, /this\.navigation\.navigationItems\.map\(/u, 'os cartões saem da mesma lista da sidebar');
});
