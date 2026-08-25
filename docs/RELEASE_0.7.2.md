# NarraHub 0.7.2

Esta versão corrige o carregamento dos estilos no aplicativo Windows instalado. O defeito afetava ao mesmo tempo a troca de tema e o layout das configurações.

## Causa raiz

O otimizador de CSS do Angular gerava o stylesheet principal com `media="print"` e usava um manipulador `onload` inline para ativá-lo. A política de segurança do Tauri bloqueia manipuladores inline. Como consequência, o WebView carregava somente uma pequena parte crítica do tema claro e ignorava o restante dos estilos.

## Correções

- O bundle de produção agora referencia o stylesheet diretamente, de forma compatível com o CSP existente.
- A política de segurança não foi enfraquecida com `unsafe-eval` ou permissão de scripts inline.
- Claro, escuro e sistema atualizam toda a interface e mantêm a preferência após reiniciar.
- O menu lateral, as abas, os cartões, os campos e os textos das configurações voltaram ao layout correto.
- A seção de inteligência local não exibe mais rótulos sobrepostos ou campos comprimidos por falta de CSS.

## Atualização

Instale a versão 0.7.2 sobre a instalação atual ou use o atualizador interno. O banco em `%APPDATA%\com.narrahub.app\narrahub.db` não deve ser removido; a atualização preserva universos, histórias e preferências.

## Validação

- Bundle Angular de produção compilado com stylesheet direto e sem `media="print"` ou `onload` inline.
- Seletores de tema escuro, navegação das configurações e ocultação das seções inativas verificados no CSS final.
- Alternância claro/escuro e persistência após recarregar verificadas em navegador real sobre a compilação de produção.
- Layout das configurações verificado nas seções Geral e Inteligência, em tema claro e escuro.
- A pipeline de release executa a mesma inspeção antes de criar os instaladores.
