// Configuração do build Android de release, com o `versionCode` calculado da versão SemVer.
//
// O Android só instala por cima quando o `versionCode` não diminui. O cálculo padrão do Tauri
// (maior·1.000.000 + menor·1.000 + patch) IGNORA o pré-release: 0.10.0-beta.1 e 0.10.0-beta.2
// sairiam com o mesmo número, e a estável 0.10.0 também — a atualização entre elas ficaria
// dependendo de o Android aceitar número igual.
//
// Aqui o pré-release entra nos dois últimos dígitos, na mesma ordem da SemVer:
//
//   núcleo · 100 +  alpha.N → N        (1..32)
//                   beta.N  → 32 + N   (33..64)
//                   rc.N    → 64 + N   (65..96)
//                   estável → 99
//
// Assim alpha < beta < rc < estável dentro da mesma versão, e qualquer versão maior vence.
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const LIMITE_DO_ANDROID = 2_100_000_000;
const CANAIS = { alpha: 0, beta: 32, rc: 64 };

export function versionCodeDe(versao) {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/u.exec(versao);
  if (!m) throw new Error(`"${versao}" não é SemVer MAIOR.MENOR.PATCH.`);
  const [maior, menor, patch] = [Number(m[1]), Number(m[2]), Number(m[3])];
  if (menor > 999 || patch > 999) throw new Error(`"${versao}": menor e patch precisam ficar abaixo de 1000.`);

  let sufixo = 99;
  if (m[4] !== undefined) {
    const pre = /^(alpha|beta|rc)\.(\d+)$/u.exec(m[4]);
    if (!pre) {
      throw new Error(`"${versao}": o pré-release precisa ser alpha.N, beta.N ou rc.N para virar versionCode.`);
    }
    const numero = Number(pre[2]);
    if (numero < 1 || numero > 32) throw new Error(`"${versao}": o número do pré-release vai de 1 a 32.`);
    sufixo = CANAIS[pre[1]] + numero;
  }

  const codigo = (maior * 1_000_000 + menor * 1_000 + patch) * 100 + sufixo;
  if (codigo > LIMITE_DO_ANDROID) throw new Error(`"${versao}" gera versionCode ${codigo}, acima do limite do Android.`);
  return codigo;
}

async function principal() {
  const { version } = JSON.parse(await readFile('package.json', 'utf8'));
  const producao = JSON.parse(await readFile('src-tauri/tauri.production.conf.json', 'utf8'));
  const versionCode = versionCodeDe(version);
  const config = {
    ...producao,
    bundle: { ...(producao.bundle || {}), android: { ...(producao.bundle?.android || {}), versionCode } },
  };
  const destino = path.resolve('src-tauri/tauri.android-release.conf.json');
  await writeFile(destino, `${JSON.stringify(config, null, 2)}\n`, 'utf8');
  console.log(`Android ${version} -> versionCode ${versionCode} (${destino})`);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await principal();
}
