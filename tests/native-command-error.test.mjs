import assert from 'node:assert/strict';
import test from 'node:test';
import { NativeCommandError, normalizeNativeCommandError } from '../src/app/core/errors/native-command-error.ts';

test('preserva tipo e mensagem enviados pelo command Tauri', () => {
  const error = normalizeNativeCommandError(
    { kind: 'conflict', message: 'Já existe um backup em andamento.' },
    'fallback',
  );
  assert.ok(error instanceof NativeCommandError);
  assert.equal(error.kind, 'conflict');
  assert.equal(error.message, 'Já existe um backup em andamento.');
});

test('normaliza rejeições legadas sem exibir object Object', () => {
  const legacy = normalizeNativeCommandError('Falha de armazenamento.', 'fallback');
  const unknown = normalizeNativeCommandError({ unexpected: true }, 'Operação indisponível.');
  assert.equal(legacy.kind, 'unavailable');
  assert.equal(legacy.message, 'Falha de armazenamento.');
  assert.equal(unknown.message, 'Operação indisponível.');
});

