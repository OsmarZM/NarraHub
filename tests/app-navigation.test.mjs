import assert from 'node:assert/strict';
import test from 'node:test';
import { buildAppPath, parseAppPath } from '../src/app/core/navigation/app-navigation.ts';

test('gera rotas de alto nível sem expor detalhes do banco', () => {
  assert.equal(buildAppPath('inicio', null), '/library');
  assert.equal(buildAppPath('configuracoes', null), '/settings');
  assert.equal(buildAppPath('timeline', 'universo com espaço'), '/workspace/universo%20com%20espa%C3%A7o/timeline');
});

test('restaura universo e feature a partir da URL', () => {
  assert.deepEqual(parseAppPath('/workspace/u-123/history?source=restart'), { navId: 'historico', universeId: 'u-123' });
  assert.deepEqual(parseAppPath('/workspace/universo%20x/entities'), { navId: 'entidades', universeId: 'universo x' });
  assert.deepEqual(parseAppPath('/rota-inexistente'), { navId: 'inicio', universeId: null });
});

