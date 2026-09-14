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

  const anexar = workflow.indexOf('gh release upload');
  const publicar = workflow.indexOf('--draft=false');
  assert.ok(anexar > 0 && publicar > anexar, 'a release só pode ser publicada depois de anexar APK e SHA');
});
