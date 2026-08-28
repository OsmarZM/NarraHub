import { inject } from '@angular/core';
import { CanDeactivateFn } from '@angular/router';
import { ManuscriptStore } from '../features/manuscript/state/manuscript.store';

/**
 * Salva o capítulo antes de sair da rota de Escrita.
 *
 * Isto NÃO substitui o `saveNow()` explícito: o guard só cobre navegação do
 * Angular. Fechar a janela, instalar atualização e restaurar backup acontecem
 * fora do Router e continuam chamando `saveNow()` por conta própria — ver a
 * regra na Fase 3.3 do plano de arquitetura.
 *
 * Nunca bloqueia a navegação: se o salvamento falhar, o store já registra o
 * erro e mostra "Erro ao salvar" na barra; prender o usuário na tela não
 * recupera o texto e só piora a situação.
 */
export const unsavedChapterGuard: CanDeactivateFn<unknown> = async () => {
  await inject(ManuscriptStore).saveNow();
  return true;
};
