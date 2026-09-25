import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { versionCodeDe } from '../scripts/prepare-android-release-config.mjs';

test('o versionCode respeita a ordem SemVer, inclusive 0.9.10 depois de 0.9.2', () => {
  // O Android recusa instalar por cima quando o versionCode diminui. A ordem destes números
  // precisa ser exatamente a ordem das versões.
  const ordem = [
    '0.9.2',
    '0.9.3',
    '0.9.10',
    '0.10.0-alpha.1',
    '0.10.0-alpha.32',
    '0.10.0-beta.1',
    '0.10.0-beta.2',
    '0.10.0-beta.10',
    '0.10.0-rc.1',
    '0.10.0',
    '0.10.1-beta.1',
    '1.0.0',
  ];
  for (let i = 1; i < ordem.length; i += 1) {
    assert.ok(
      versionCodeDe(ordem[i]) > versionCodeDe(ordem[i - 1]),
      `${ordem[i]} (${versionCodeDe(ordem[i])}) precisa ter versionCode maior que ${ordem[i - 1]} (${versionCodeDe(ordem[i - 1])})`,
    );
  }
});

test('betas da mesma versao nao colidem, que era o defeito do calculo padrao', () => {
  assert.notEqual(versionCodeDe('0.10.0-beta.1'), versionCodeDe('0.10.0-beta.2'));
  assert.notEqual(versionCodeDe('0.10.0-beta.2'), versionCodeDe('0.10.0'));
});

test('recusa o que nao vira versionCode com seguranca', () => {
  for (const errada of ['0.9', '0.9.x', '0.10.0-preview.1', '0.10.0-beta', '0.10.0-beta.0', '0.10.0-beta.33', '0.1000.0', '22.0.0']) {
    assert.throws(() => versionCodeDe(errada), undefined, `devia recusar ${errada}`);
  }
});

test('a release publica APK com nome estavel e o SHA-256 ao lado, antes de publicar', () => {
  // O app Android procura exatamente estes dois nomes (atualizacao_android.rs). Se o workflow
  // mudar o nome, o app deixa de achar atualização sem erro nenhum.
  const workflow = readFileSync(new URL('../.github/workflows/release-windows.yml', import.meta.url), 'utf8');
  const rust = readFileSync(new URL('../src-tauri/src/application/atualizacao_android.rs', import.meta.url), 'utf8');

  const apk = rust.match(/pub const ASSET_APK: &str = "([^"]+)";/u)?.[1];
  const sha = rust.match(/pub const ASSET_SHA: &str = "([^"]+)";/u)?.[1];
  assert.equal(apk, 'NarraHub-Android.apk');
  assert.equal(sha, 'NarraHub-Android.apk.sha256');

  assert.ok(workflow.includes(apk), 'o workflow não publica o APK com o nome que o app procura');
  assert.ok(workflow.includes(sha), 'o workflow não publica o SHA-256 com o nome que o app procura');
  assert.ok(/sha256sum\s+NarraHub-Android\.apk\s*>\s*NarraHub-Android\.apk\.sha256/u.test(workflow), 'o SHA-256 precisa ser calculado sobre o APK final');
  assert.ok(/apksigner"?\s+verify/u.test(workflow), 'a assinatura precisa ser verificada antes de publicar');

  for (const nome of ['release', 'android-prerelease']) {
    const bloco = jobDoWorkflow(workflow, nome);
    const anexar = bloco.indexOf('gh release upload');
    const publicar = bloco.indexOf('--draft=false');
    assert.ok(anexar > 0 && publicar > anexar, `${nome}: a release só pode ser publicada depois de anexar APK e SHA`);
  }
});

function jobDoWorkflow(workflow, nome) {
  const inicio = workflow.indexOf(`\n  ${nome}:\n`);
  assert.ok(inicio >= 0, `job ${nome} não existe`);
  const resto = workflow.slice(inicio + 1);
  const proximo = resto.slice(1).search(/\n  [a-z][a-z0-9-]*:\n/u);
  return proximo < 0 ? resto : resto.slice(0, proximo + 1);
}

test('pré-release é só Android; Windows só recebe versão estável', () => {
  // O Windows Installer não distingue 0.10.0-beta.1 de 0.10.0. Uma beta com MSI criaria dívida de
  // versão no Windows; por isso o caminho é escolhido pela versão, e cada caminho recusa o outro.
  const workflow = readFileSync(new URL('../.github/workflows/release-windows.yml', import.meta.url), 'utf8');
  const windows = jobDoWorkflow(workflow, 'release');
  const pre = jobDoWorkflow(workflow, 'android-prerelease');

  assert.match(windows, /if: needs\.android\.outputs\.pre == 'false'/u);
  assert.match(windows, /prerelease: false/u);
  assert.match(pre, /if: needs\.android\.outputs\.pre == 'true'/u);
  assert.match(pre, /--prerelease/u);
  assert.doesNotMatch(pre, /tauri-action|TAURI_SIGNING|latest\.json|msi|nsis/iu, 'a pré-release não pode gerar nada do Windows');
  assert.match(pre, /\*\) echo "::error::\$VERSAO é estável/u, 'o job de pré-release recusa versão estável');
  assert.doesNotMatch(workflow, /wix/iu, 'nenhum ajuste de versão do MSI: beta não passa pelo Windows');
});

// I-BUG-06 (Etapa I, achado em aparelho físico): o arranque só procurava atualização quando o
// atualizador do DESKTOP estava configurado, o que nunca é verdade no Android — a beta nova só
// aparecia indo em Configurações. O canal do Android tem de bastar para o arranque procurar.
test('o arranque procura atualização também pelo canal do Android', () => {
  const arranque = readFileSync(new URL('../src/app/bootstrap/app-bootstrap.service.ts', import.meta.url), 'utf8');
  assert.match(arranque, /if \(await this\.settings\.shouldCheckForUpdatesOnStartup\(\)\)/u);
  assert.doesNotMatch(arranque, /if \(await this\.settings\.isUpdateConfigured\(\)\)/u);
  const store = readFileSync(new URL('../src/app/features/settings/state/settings.store.ts', import.meta.url), 'utf8');
  const metodo = store.slice(store.indexOf('async shouldCheckForUpdatesOnStartup'));
  assert.match(metodo.slice(0, 300), /this\.androidUpdate\.supported\(\)/u);
});

// I-BUG-09 (Etapa I, achado em aparelho físico): o reqwest 0.13 com `rustls` usa o verificador da
// plataforma, que no Android exige inicialização por JNI — sem ela, a primeira requisição HTTPS do
// atualizador entra em pânico e a atualização nunca aparece. O cliente do Android usa TLS com as
// raízes embutidas; o `cfg` não compila no desktop, então o contrato é textual.
test('o cliente HTTPS do atualizador Android não depende do verificador da plataforma', () => {
  const fonte = readFileSync(new URL('../src-tauri/src/application/atualizacao_android.rs', import.meta.url), 'utf8');
  const cliente = fonte.slice(fonte.indexOf('fn cliente('), fonte.indexOf('fn tls_com_raizes_embutidas'));
  assert.match(cliente, /#\[cfg\(target_os = "android"\)\]\s*let construtor = construtor\.tls_backend_preconfigured\(tls_com_raizes_embutidas\(\)\?\);/u);
  assert.match(fonte, /webpki_roots::TLS_SERVER_ROOTS/u);
});
