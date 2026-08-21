import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { FileShareStore, validateEnvelope } from '../src/share-store.mjs';

const envelope = { version: 1, algorithm: 'A256GCM', iv: 'abcdefghijklmnop', ciphertext: 'abcdefghijklmnopqrstuvwx', expiresInDays: 7 };

test('persiste, recupera e revoga apenas com o token correto', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'narrahub-share-'));
  try {
    const store = new FileShareStore(directory);
    await store.init();
    const created = await store.create(envelope);
    assert.equal((await store.get(created.id)).envelope.ciphertext, envelope.ciphertext);
    assert.equal(await store.revoke(created.id, 'token-incorreto'), false);
    assert.equal(await store.revoke(created.id, created.revokeToken), true);
    assert.equal(await store.get(created.id), null);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('rejeita conteúdo aberto ou expiração fora do contrato', () => {
  assert.throws(() => validateEnvelope({ ...envelope, algorithm: 'none' }), /algoritmo/u);
  assert.throws(() => validateEnvelope({ ...envelope, expiresInDays: 365 }), /Expiração/u);
  assert.throws(() => validateEnvelope({ ...envelope, ciphertext: '<script>alert(1)</script>' }), /cifrado/u);
});

test('visualizador aceita pacotes selecionados sem injetar HTML arbitrário', async () => {
  const viewer = await readFile(new URL('../public/viewer.js', import.meta.url), 'utf8');
  assert.match(viewer, /payload\.version === 2 && payload\.kind === 'bundle'/u);
  assert.match(viewer, /const allowed = new Set/u);
  assert.doesNotMatch(viewer, /\.innerHTML\s*=/u);
});
