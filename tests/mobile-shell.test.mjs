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

test('gesto a 60 fps: por quadro só transform, opacity, z-index e visibilidade', () => {
  // Cada propriedade que dispara layout (top, left, width, height, margin, padding) ou uma
  // variável CSS na raiz, escrita a cada pointermove, custa o quadro inteiro num Android modesto.
  const ts = ler('../src/app/shell/mobile-navigation/mobile-navigation.component.ts');
  const render = ts.slice(ts.indexOf('  private render(): void {'), ts.indexOf('\n  }\n', ts.indexOf('  private render(): void {')));
  assert.ok(render.length > 200, 'render() não encontrado');
  const escritas = [...render.matchAll(/\.style\.([a-zA-Z]+)\s*=/gu)].map((m) => m[1]);
  assert.ok(escritas.includes('transform') && escritas.includes('opacity'));
  const permitidas = new Set(['transform', 'opacity', 'zIndex', 'visibility', 'willChange']);
  for (const propriedade of escritas) assert.ok(permitidas.has(propriedade), `render() escreve style.${propriedade} por quadro`);
  assert.doesNotMatch(render, /setProperty\(|getBoundingClientRect|innerWidth|offsetWidth|offsetHeight|clientWidth/u, 'render() lê layout ou escreve variável CSS');

  // O pointermove não escreve DOM nem lê layout: só atualiza números e pede um quadro.
  const move = ts.slice(ts.indexOf('const move = (event: PointerEvent) => {'), ts.indexOf("listenWindow('pointermove', move);"));
  assert.ok(move.length > 100, 'handler de pointermove não encontrado');
  assert.doesNotMatch(move, /\.style\.|classList|getBoundingClientRect|innerWidth|setProperty/u);
  assert.match(move, /this\.requestRender\(\)/u);
  // E roda fora da zona do Angular.
  assert.match(ts, /this\.zone\.runOutsideAngular\(\(\) => this\.bindPointer\(\)\)/u);
});

test('alça perceptível, alvo de polegar, e a faixa sem "voltar" do Android no mesmo lugar', () => {
  const css = ler('../src/app/shell/mobile-navigation/mobile-navigation.component.css');
  const regra = css.slice(css.indexOf('.nh-mnav-handle {'), css.indexOf('}', css.indexOf('.nh-mnav-handle {')));
  const px = (prop) => Number(regra.match(new RegExp(`\\n\\s*${prop}:\\s*(\\d+)px`, 'u'))?.[1]);
  assert.ok(px('width') >= 44 && px('height') >= 44, 'alvo de toque menor que 44px');
  const topo = Number(regra.match(/top:\s*(\d+)%/u)?.[1]);
  assert.ok(topo >= 55 && topo <= 65, `alça fora da faixa do polegar: ${topo}%`);

  const html = ler('../src/app/shell/mobile-navigation/mobile-navigation.component.html');
  assert.match(html, /class="nh-mnav-grip"[^>]*><i><\/i><i><\/i><i><\/i><\/span>/u, 'a alça tem três barras');
  // O feedback do toque não anima largura nem altura.
  const grip = css.slice(css.indexOf('.nh-mnav-grip {'), css.indexOf('}', css.indexOf('.nh-mnav-grip {')));
  assert.doesNotMatch(grip, /transition:[^;]*(width|height)/u);

  const activity = ler('../src-tauri/gen/android/app/src/main/java/com/narrahub/app/MainActivity.kt');
  const centro = Number(activity.match(/ALCA_CENTRO = (0\.\d+)f/u)?.[1]);
  assert.equal(Math.round(centro * 100), topo, 'o MainActivity desliga o "voltar" em outra altura que não a da alça');
  const faixa = Number(activity.match(/val faixa = \((\d+) \* dp\)/u)?.[1]);
  assert.ok(faixa >= px('width') && faixa <= 56, `faixa sem "voltar" (${faixa}dp) deve cobrir só a alça (${px('width')}px)`);
});

test('toque na alça abre, peteleco curto abre, e a dica aparece uma vez por versão', () => {
  const ts = ler('../src/app/shell/mobile-navigation/mobile-navigation.component.ts');
  assert.match(ts, /const tocou = Math\.abs\(event\.clientX - gesture\.startX\) < TAP_SLOP;/u);
  assert.match(ts, /this\.animateOpen\(tocou \|\| settleOpen\(this\.open, -velocity\.x\) \? 1 : 0/u);
  assert.match(ts, /MOBILE_NAV_HINT_KEY = 'narrahub\.mobileNavigationHintSeen'/u);
  assert.match(ts, /MOBILE_NAV_HINT_VERSION = '\d+'/u);
  assert.match(ler('../src-tauri/gen/android/app/src/main/AndroidManifest.xml'), /android\.permission\.VIBRATE/u);
});
